//! Local transcription with whisper.cpp's `whisper-cli`. Audio never leaves the machine.
//! The model (ggml base.en, about 150 MB) is downloaded once, with progress, and cached.
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use futures_util::StreamExt;
use serde_json::Value;
use tokio::io::AsyncWriteExt;

use crate::brand::BRAND;
use crate::cuts::Word;
use crate::library::{self, Transcript};
use crate::media;

const MODEL_FILE: &str = "ggml-base.en.bin";
const MODEL_URL: &str =
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin";

pub fn model_path() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(&BRAND.slug)
        .join("models")
        .join(MODEL_FILE)
}

pub fn model_ready() -> bool {
    library::exists(&model_path())
}

pub fn cli_ready() -> bool {
    media::tool("whisper-cli").exists()
}

/// Downloads the model if missing. `progress(downloaded, total)` is called as bytes arrive.
pub async fn ensure_model(progress: impl Fn(u64, u64)) -> Result<PathBuf> {
    let path = model_path();
    if model_ready() {
        return Ok(path);
    }
    std::fs::create_dir_all(path.parent().context("model dir")?)?;
    let res = reqwest::get(MODEL_URL)
        .await
        .context("could not download the speech model")?
        .error_for_status()?;
    let total = res.content_length().unwrap_or(0);
    let tmp = path.with_extension("part");
    let mut file = tokio::fs::File::create(&tmp).await?;
    let mut got = 0u64;
    let mut stream = res.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk).await?;
        got += chunk.len() as u64;
        progress(got, total);
    }
    file.flush().await?;
    tokio::fs::rename(&tmp, &path).await?;
    Ok(path)
}

pub async fn transcribe(id: &str, progress: impl Fn(u64, u64)) -> Result<Transcript> {
    if !cli_ready() {
        bail!("whisper-cli is not installed. On macOS: brew install whisper-cpp");
    }
    let model = ensure_model(progress).await?;
    let wav = library::dir(id).join("audio.wav");
    media::extract_wav(&library::video_path(id), &wav).await?;
    let base = library::dir(id).join("whisper");
    let res = media::run(
        "whisper-cli",
        &[
            "-m",
            &model.to_string_lossy(),
            "-f",
            &wav.to_string_lossy(),
            "-oj",
            "-of",
            &base.to_string_lossy(),
            "-ml",
            "1",
            "-sow",
            "-np",
        ],
    )
    .await;
    let _ = tokio::fs::remove_file(&wav).await;
    res?;
    let json_path = base.with_extension("json");
    let raw = tokio::fs::read(&json_path)
        .await
        .context("whisper produced no output")?;
    let _ = tokio::fs::remove_file(&json_path).await;
    let t = parse_whisper(&serde_json::from_slice(&raw)?);
    library::save_transcript(id, &t)?;
    Ok(t)
}

/// whisper-cli with `-ml 1 -sow` emits one segment per word, with millisecond offsets.
pub fn parse_whisper(v: &Value) -> Transcript {
    let words: Vec<Word> = v["transcription"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|seg| {
            let text = seg["text"].as_str()?.trim().to_string();
            if text.is_empty() || text.starts_with('[') || text.starts_with('(') {
                return None; // [BLANK_AUDIO], (music) and similar
            }
            Some(Word {
                start: seg["offsets"]["from"].as_f64()? / 1000.0,
                end: seg["offsets"]["to"].as_f64()? / 1000.0,
                text,
            })
        })
        .collect();
    let text = words
        .iter()
        .map(|w| w.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    Transcript { words, text }
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_word_segments_and_skips_markers() {
        let v = serde_json::json!({"transcription": [
            {"offsets": {"from": 0, "to": 400}, "text": " Hello"},
            {"offsets": {"from": 400, "to": 900}, "text": " [BLANK_AUDIO]"},
            {"offsets": {"from": 900, "to": 1300}, "text": " world"}
        ]});
        let t = super::parse_whisper(&v);
        assert_eq!(t.text, "Hello world");
        assert_eq!(t.words[1].start, 0.9);
    }
}
