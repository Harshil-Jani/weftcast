//! Claude features behind one interface with two providers:
//! - `Cli`: the user's own installed `claude` CLI in headless mode, so it runs on whatever
//!   login that user already has. We never see or store that login.
//! - `Api`: the Anthropic Messages API with the user's own ANTHROPIC_API_KEY.
use std::process::Stdio;

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::AsyncWriteExt;

use crate::brand::BRAND;
use crate::library::{Chapter, Transcript};

pub const MODEL: &str = "claude-opus-5-5";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    #[default]
    Cli,
    Api,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Settings {
    #[serde(default)]
    pub provider: Provider,
    /// Optional. The ANTHROPIC_API_KEY environment variable wins when set.
    #[serde(default)]
    pub api_key: Option<String>,
}

fn settings_path() -> std::path::PathBuf {
    crate::library::root().join(".settings.json")
}

pub fn load_settings() -> Settings {
    std::fs::read(settings_path())
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save_settings(s: &Settings) -> Result<()> {
    std::fs::create_dir_all(crate::library::root())?;
    std::fs::write(settings_path(), serde_json::to_vec_pretty(s)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(settings_path(), std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn api_key(s: &Settings) -> Option<String> {
    std::env::var("ANTHROPIC_API_KEY")
        .ok()
        .filter(|k| !k.is_empty())
        .or_else(|| s.api_key.clone().filter(|k| !k.is_empty()))
}

#[derive(Debug, Serialize)]
pub struct ProviderStatus {
    pub provider: Provider,
    pub cli_installed: bool,
    pub cli_version: Option<String>,
    pub api_key_set: bool,
    pub model: &'static str,
}

pub async fn status() -> ProviderStatus {
    let s = load_settings();
    let v = tokio::process::Command::new("claude")
        .arg("--version")
        .output()
        .await
        .ok()
        .filter(|o| o.status.success());
    ProviderStatus {
        provider: s.provider,
        cli_installed: v.is_some(),
        cli_version: v.map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()),
        api_key_set: api_key(&s).is_some(),
        model: MODEL,
    }
}

/// One call to Claude that must return JSON matching `schema`.
pub async fn ask_json(system: &str, user: &str, schema: &Value) -> Result<Value> {
    let s = load_settings();
    match s.provider {
        Provider::Api => {
            let key = api_key(&s).ok_or_else(|| anyhow!("No API key. Set ANTHROPIC_API_KEY or add one in Settings, or switch to the claude CLI."))?;
            api_json(&key, system, user, schema).await
        }
        Provider::Cli => cli_json(system, user, schema).await,
    }
}

async fn api_json(key: &str, system: &str, user: &str, schema: &Value) -> Result<Value> {
    let body = json!({
        "model": MODEL,
        "max_tokens": 16000,
        "system": system,
        "output_config": { "effort": "medium", "format": { "type": "json_schema", "schema": schema } },
        "fallbacks": "default",
        "messages": [{ "role": "user", "content": user }],
    });
    let res = reqwest::Client::new()
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", key)
        .header("anthropic-version", "2023-06-01")
        .header("anthropic-beta", "server-side-fallback-2026-07-01")
        .timeout(std::time::Duration::from_secs(600))
        .json(&body)
        .send()
        .await
        .context("could not reach the Anthropic API")?;
    let status = res.status();
    let v: Value = res.json().await.context("bad API response")?;
    if !status.is_success() {
        bail!(
            "Anthropic API {}: {}",
            status.as_u16(),
            v["error"]["message"].as_str().unwrap_or("unknown error")
        );
    }
    match v["stop_reason"].as_str() {
        Some("refusal") => bail!("Claude declined this request."),
        Some("max_tokens") => bail!("Claude's answer was cut off. Try a shorter video."),
        _ => {}
    }
    let text: String = v["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|b| b["type"] == "text")
        .filter_map(|b| b["text"].as_str())
        .collect();
    serde_json::from_str(&text).context("Claude did not return valid JSON")
}

async fn cli_json(system: &str, user: &str, schema: &Value) -> Result<Value> {
    let prompt = format!(
        "{system}\n\nReply with ONLY a JSON object, no prose and no code fences, matching this JSON Schema:\n{}\n\n{user}",
        serde_json::to_string(schema)?
    );
    let mut child = tokio::process::Command::new("claude")
        .args(["-p", "--output-format", "json"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("The claude CLI is not installed. Install Claude Code, run `claude` once to log in, or switch to an API key in Settings.")?;
    child
        .stdin
        .take()
        .context("stdin")?
        .write_all(prompt.as_bytes())
        .await?;
    let out = tokio::time::timeout(
        std::time::Duration::from_secs(600),
        child.wait_with_output(),
    )
    .await
    .context("the claude CLI timed out")??;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let envelope: Value = serde_json::from_str(stdout.trim()).map_err(|_| {
        anyhow!(
            "The claude CLI failed. Is it logged in? Run `claude` once in a terminal.\n{}",
            String::from_utf8_lossy(&out.stderr).trim()
        )
    })?;
    if envelope["is_error"].as_bool() == Some(true) {
        bail!(
            "claude CLI: {}",
            envelope["result"].as_str().unwrap_or("error")
        );
    }
    extract_json(envelope["result"].as_str().unwrap_or_default())
}

/// The CLI path has no schema enforcement, so pull the first JSON object out of the reply.
pub fn extract_json(text: &str) -> Result<Value> {
    let t = text
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    if let Ok(v) = serde_json::from_str(t) {
        return Ok(v);
    }
    let (a, b) = (t.find('{'), t.rfind('}'));
    match (a, b) {
        (Some(a), Some(b)) if b > a => {
            serde_json::from_str(&t[a..=b]).context("Claude did not return valid JSON")
        }
        _ => bail!("Claude did not return JSON"),
    }
}

/// Transcript with a timestamp every few words, so Claude can cite times.
pub fn timed_transcript(t: &Transcript) -> String {
    let mut out = String::new();
    let mut last = -10.0;
    for w in &t.words {
        if w.start - last >= 5.0 {
            out += &format!("\n[{}] ", mmss(w.start));
            last = w.start;
        }
        out += w.text.trim();
        out.push(' ');
    }
    out.trim().to_string()
}

pub fn mmss(t: f64) -> String {
    let s = t.max(0.0).round() as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Insights {
    pub title: String,
    pub summary: String,
    pub chapters: Vec<Chapter>,
}

pub fn insights_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["title", "summary", "chapters"],
        "properties": {
            "title": { "type": "string" },
            "summary": { "type": "string" },
            "chapters": { "type": "array", "items": {
                "type": "object", "additionalProperties": false, "required": ["start", "title"],
                "properties": { "start": { "type": "number" }, "title": { "type": "string" } }
            }}
        }
    })
}

/// Checks Claude's chapters against the real video: sorted, inside the video, first at 0, no tiny chapters.
pub fn validate_insights(v: Value, duration: f64) -> Result<Insights> {
    let mut ins: Insights = serde_json::from_value(v)
        .context("Claude's answer is missing title, summary or chapters")?;
    ins.title = ins
        .title
        .trim()
        .trim_matches('"')
        .chars()
        .take(90)
        .collect();
    anyhow::ensure!(!ins.title.is_empty(), "empty title");
    ins.chapters.retain(|c| {
        c.start.is_finite() && c.start >= 0.0 && c.start < duration && !c.title.trim().is_empty()
    });
    ins.chapters.sort_by(|a, b| a.start.total_cmp(&b.start));
    let mut kept: Vec<Chapter> = Vec::new();
    for c in ins.chapters {
        if kept.last().is_none_or(|l| c.start - l.start >= 5.0) {
            kept.push(c);
        }
    }
    if let Some(first) = kept.first_mut() {
        first.start = 0.0;
    }
    if kept.len() < 2 {
        kept.clear(); // a single chapter adds nothing
    }
    ins.chapters = kept;
    Ok(ins)
}

pub async fn insights(t: &Transcript, duration: f64) -> Result<Insights> {
    let system = format!(
        "You help people share screen recordings made with {}. Write plainly, no hype, no emojis.",
        BRAND.name
    );
    let user = format!(
        "Video length: {}. Transcript with [m:ss] markers:\n\n{}\n\n\
         Return:\n- title: under 70 characters, specific to what is shown\n- summary: 2 to 4 sentences\n\
         - chapters: 2 to 8 sections, each with start in SECONDS (number) and a short title. \
         Use none if the video is under 45 seconds.",
        mmss(duration),
        timed_transcript(t)
    );
    validate_insights(
        ask_json(&system, &user, &insights_schema()).await?,
        duration,
    )
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Answer {
    pub answer: String,
    pub timestamps: Vec<f64>,
}

pub async fn ask(t: &Transcript, title: &str, question: &str) -> Result<Answer> {
    let schema = json!({
        "type": "object", "additionalProperties": false, "required": ["answer", "timestamps"],
        "properties": {
            "answer": { "type": "string" },
            "timestamps": { "type": "array", "items": { "type": "number" } }
        }
    });
    let system = format!(
        "You answer questions about one video recorded with {}, using only its transcript. \
         If the transcript does not contain the answer, say so. Cite moments as [m:ss] in the answer.",
        BRAND.name
    );
    let q: String = question.chars().take(500).collect();
    let user = format!(
        "Video title: {title}\nTranscript:\n{}\n\nQuestion: {q}\n\n\
         Return answer (under 120 words) and timestamps: the seconds of the most relevant moments, at most 3.",
        timed_transcript(t)
    );
    let mut a: Answer = serde_json::from_value(ask_json(&system, &user, &schema).await?)?;
    a.timestamps.retain(|s| s.is_finite() && *s >= 0.0);
    a.timestamps.truncate(3);
    Ok(a)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_and_repairs_chapters() {
        let v = json!({"title": " \"Demo\" ", "summary": "s", "chapters": [
            {"start": 30, "title": "Middle"}, {"start": 2, "title": "Intro"},
            {"start": 32, "title": "Too close"}, {"start": 500, "title": "Past the end"},
            {"start": 60, "title": "  "}
        ]});
        let ins = validate_insights(v, 120.0).unwrap();
        assert_eq!(ins.title, "Demo");
        let starts: Vec<f64> = ins.chapters.iter().map(|c| c.start).collect();
        assert_eq!(starts, vec![0.0, 30.0]);
    }

    #[test]
    fn single_chapter_is_dropped() {
        let v = json!({"title": "t", "summary": "s", "chapters": [{"start": 0, "title": "All"}]});
        assert!(validate_insights(v, 60.0).unwrap().chapters.is_empty());
    }

    #[test]
    fn rejects_missing_fields() {
        assert!(validate_insights(json!({"title": "t"}), 10.0).is_err());
    }

    #[test]
    fn extracts_json_from_chatty_cli_output() {
        assert_eq!(extract_json("```json\n{\"a\":1}\n```").unwrap()["a"], 1);
        assert_eq!(
            extract_json("Sure! {\"a\": 2} hope that helps").unwrap()["a"],
            2
        );
        assert!(extract_json("no json here").is_err());
    }

    #[test]
    fn mmss_formats() {
        assert_eq!(mmss(0.0), "0:00");
        assert_eq!(mmss(75.4), "1:15");
    }
}
