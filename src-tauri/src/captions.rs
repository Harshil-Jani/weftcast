//! Caption translation. Claude translates line by line; the original timings are kept exactly,
//! and each language is saved as its own track next to the video.
use anyhow::{Context, Result, ensure};
use serde_json::json;

use crate::ai;
use crate::library;

pub type Cue = (f64, f64, String);

/// Languages offered in the picker: (code, English name).
pub const LANGUAGES: &[(&str, &str)] = &[
    ("hi", "Hindi"),
    ("es", "Spanish"),
    ("fr", "French"),
    ("de", "German"),
    ("pt", "Portuguese"),
    ("ar", "Arabic"),
    ("ja", "Japanese"),
    ("zh", "Chinese"),
    ("ko", "Korean"),
    ("it", "Italian"),
    ("nl", "Dutch"),
    ("ru", "Russian"),
    ("tr", "Turkish"),
    ("id", "Indonesian"),
    ("bn", "Bengali"),
    ("ta", "Tamil"),
    ("te", "Telugu"),
    ("mr", "Marathi"),
    ("gu", "Gujarati"),
];

pub fn language_name(code: &str) -> Option<&'static str> {
    LANGUAGES.iter().find(|(c, _)| *c == code).map(|(_, n)| *n)
}

pub fn track_path(id: &str, lang: &str) -> std::path::PathBuf {
    library::dir(id).join(format!("captions.{lang}.vtt"))
}

fn parse_stamp(s: &str) -> Option<f64> {
    let s = s.trim().replace(',', ".");
    let parts: Vec<&str> = s.split(':').collect();
    let (h, m, sec) = match parts.as_slice() {
        [h, m, s] => (
            h.parse::<f64>().ok()?,
            m.parse::<f64>().ok()?,
            s.parse::<f64>().ok()?,
        ),
        [m, s] => (0.0, m.parse::<f64>().ok()?, s.parse::<f64>().ok()?),
        _ => return None,
    };
    Some(h * 3600.0 + m * 60.0 + sec)
}

/// Reads SRT or VTT cues.
pub fn parse(text: &str) -> Vec<Cue> {
    let mut out = Vec::new();
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        let Some((a, b)) = line.split_once("-->") else {
            continue;
        };
        let (Some(start), Some(end)) = (
            parse_stamp(a),
            parse_stamp(b.split_whitespace().next().unwrap_or("")),
        ) else {
            continue;
        };
        let mut body = Vec::new();
        while let Some(l) = lines.peek() {
            if l.trim().is_empty() {
                break;
            }
            body.push(lines.next().unwrap().trim().to_string());
        }
        out.push((start, end, body.join(" ")));
    }
    out
}

fn stamp(t: f64) -> String {
    let ms = (t.max(0.0) * 1000.0).round() as u64;
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        ms / 3_600_000,
        ms / 60_000 % 60,
        ms / 1000 % 60,
        ms % 1000
    )
}

pub fn to_vtt(cues: &[Cue]) -> String {
    let mut s = String::from("WEBVTT\n\n");
    for (a, b, t) in cues {
        s += &format!("{} --> {}\n{}\n\n", stamp(*a), stamp(*b), t);
    }
    s
}

/// Puts translated text onto the original timings; the counts must match exactly.
pub fn retime(original: &[Cue], translated: &[String]) -> Result<Vec<Cue>> {
    ensure!(
        original.len() == translated.len(),
        "got {} translated lines for {} captions",
        translated.len(),
        original.len()
    );
    Ok(original
        .iter()
        .zip(translated)
        .map(|((a, b, _), t)| (*a, *b, t.trim().to_string()))
        .collect())
}

pub async fn translate(id: &str, lang: &str) -> Result<()> {
    let name = language_name(lang).context("unsupported language")?;
    let t = library::load_transcript(id).context("Transcribe the video first.")?;
    let cues = library::cues(&t.words);
    ensure!(!cues.is_empty(), "the transcript is empty");
    let schema = json!({"type": "object", "additionalProperties": false, "required": ["lines"],
        "properties": {"lines": {"type": "array", "items": {"type": "string"}}}});
    let system = format!(
        "You translate video captions into {name}. Translate each numbered line on its own, keep the count \
         and order exactly, keep it short enough to read as a caption, and keep product names as they are."
    );
    let mut out = Vec::with_capacity(cues.len());
    // ponytail: 80 lines per call keeps each request small; raise it if calls ever feel slow.
    for chunk in cues.chunks(80) {
        let numbered: String = chunk
            .iter()
            .enumerate()
            .map(|(i, (_, _, text))| format!("{}. {text}\n", i + 1))
            .collect();
        let raw = ai::ask_json(
            &system,
            &format!("Return exactly {} lines:\n{numbered}", chunk.len()),
            &schema,
        )
        .await?;
        let lines: Vec<String> =
            serde_json::from_value(raw["lines"].clone()).context("bad translation reply")?;
        out.extend(retime(chunk, &lines)?);
    }
    std::fs::write(track_path(id, lang), to_vtt(&out))?;
    let mut v = library::load(id)?;
    if !v.caption_langs.iter().any(|l| l == lang) {
        v.caption_langs.push(lang.to_string());
    }
    library::save(&v)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cuts::Word;

    #[test]
    fn srt_and_vtt_round_trip_keeps_timings() {
        let words: Vec<Word> = [
            (0.05, 0.4, "Hello"),
            (0.4, 0.9, "team."),
            (3.0, 3.5, "Next."),
        ]
        .iter()
        .map(|(a, b, t)| Word {
            start: *a,
            end: *b,
            text: t.to_string(),
        })
        .collect();
        let original = library::cues(&words);
        for text in [library::to_srt(&words), library::to_vtt(&words)] {
            let parsed = parse(&text);
            assert_eq!(parsed.len(), original.len());
            for (p, o) in parsed.iter().zip(&original) {
                assert!((p.0 - o.0).abs() < 0.001 && (p.1 - o.1).abs() < 0.001);
                assert_eq!(p.2, o.2);
            }
        }
        let hi = retime(&original, &["नमस्ते टीम।".into(), "आगे।".into()]).unwrap();
        let again = parse(&to_vtt(&hi));
        assert_eq!((again[1].0, again[1].1), (original[1].0, original[1].1));
        assert_eq!(again[0].2, "नमस्ते टीम।");
    }

    #[test]
    fn retime_rejects_a_count_mismatch() {
        assert!(retime(&[(0.0, 1.0, "a".into())], &[]).is_err());
    }
}
