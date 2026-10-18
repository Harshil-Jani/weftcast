//! Local transcription with whisper.cpp compiled in (whisper-rs, Metal on Apple Silicon).
//! Audio never leaves the machine. The model downloads once (checksum verified) into app data,
//! or installs from a file for offline setups. Long videos are processed in chunks.
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, bail, ensure};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use crate::brand::BRAND;
use crate::cuts::Word;
use crate::library::{self, Transcript};
use crate::media;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Model {
    #[default]
    BaseEn,
    SmallEn,
}

impl Model {
    fn file(self) -> &'static str {
        match self {
            Model::BaseEn => "ggml-base.en.bin",
            Model::SmallEn => "ggml-small.en.bin",
        }
    }
    /// sha256 of the published file (Hugging Face LFS id), pinned.
    fn sha256(self) -> &'static str {
        match self {
            Model::BaseEn => "a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002",
            Model::SmallEn => "c6138d6d58ecc8322097e0f987c32f1be8bb0a18532a3f88f734d1bbf9c41e5d",
        }
    }
    pub fn size_mb(self) -> u64 {
        match self {
            Model::BaseEn => 148,
            Model::SmallEn => 488,
        }
    }
    fn url(self) -> String {
        format!(
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{}",
            self.file()
        )
    }
}

pub fn models_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(&BRAND.identifier)
        .join("models")
}

pub fn model_path(m: Model) -> PathBuf {
    models_dir().join(m.file())
}

pub fn model_ready(m: Model) -> bool {
    library::exists(&model_path(m))
}

pub fn selected() -> Model {
    crate::ai::load_settings().whisper_model
}

/// Called as (stage, done, total) while downloading or transcribing.
pub type Progress = Arc<dyn Fn(&str, u64, u64) + Send + Sync>;

/// Downloads the model if missing, verifying its checksum before it is used.
pub async fn ensure_model(m: Model, progress: &Progress) -> Result<PathBuf> {
    let path = model_path(m);
    if model_ready(m) {
        return Ok(path);
    }
    std::fs::create_dir_all(models_dir())?;
    let res = reqwest::get(m.url())
        .await
        .context("could not download the speech model")?
        .error_for_status()?;
    let total = res.content_length().unwrap_or(m.size_mb() << 20);
    let tmp = path.with_extension("part");
    let mut file = tokio::fs::File::create(&tmp).await?;
    let mut hash = Sha256::new();
    let (mut got, mut last) = (0u64, 0u64);
    let mut stream = res.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        hash.update(&chunk);
        file.write_all(&chunk).await?;
        got += chunk.len() as u64;
        if got - last > 2 << 20 {
            progress("downloading speech model", got, total);
            last = got;
        }
    }
    file.flush().await?;
    let digest = format!("{:x}", hash.finalize());
    if digest != m.sha256() {
        let _ = tokio::fs::remove_file(&tmp).await;
        bail!("the downloaded speech model failed its checksum; try again");
    }
    tokio::fs::rename(&tmp, &path).await?;
    Ok(path)
}

/// Offline install: copies a model file the user already has, after checking it is the right one.
pub async fn install_model_from(file: &Path, m: Model) -> Result<()> {
    let bytes = tokio::fs::read(file)
        .await
        .context("could not read that file")?;
    ensure!(
        format!("{:x}", Sha256::digest(&bytes)) == m.sha256(),
        "that file is not {} (checksum does not match)",
        m.file()
    );
    std::fs::create_dir_all(models_dir())?;
    tokio::fs::write(model_path(m), bytes).await?;
    Ok(())
}

/// 16 kHz mono f32 samples of `video` from `start` for `len` seconds, decoded by ffmpeg.
async fn samples(video: &Path, start: f64, len: f64) -> Result<Vec<f32>> {
    let out = tokio::process::Command::new(media::tool("ffmpeg"))
        .args([
            "-v",
            "error",
            "-ss",
            &format!("{start:.3}"),
            "-t",
            &format!("{len:.3}"),
            "-i",
        ])
        .arg(video)
        .args(["-vn", "-ac", "1", "-ar", "16000", "-f", "f32le", "-"])
        .output()
        .await
        .context("ffmpeg is missing")?;
    ensure!(
        out.status.success(),
        "could not read the audio: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(out
        .stdout
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b))
        .collect())
}

const CHUNK_SECS: f64 = 600.0;

pub async fn transcribe(id: &str, progress: Progress) -> Result<Transcript> {
    let model = ensure_model(selected(), &progress).await?;
    let video = library::video_path(id);
    let duration = media::duration(&video).await?;
    let chunks = (duration / CHUNK_SECS).ceil().max(1.0) as u64;
    let ctx = {
        let model = model.clone();
        tokio::task::spawn_blocking(move || {
            whisper_rs::install_logging_hooks();
            whisper_rs::WhisperContext::new_with_params(
                &model,
                whisper_rs::WhisperContextParameters::default(),
            )
        })
        .await?
        .context("could not load the speech model")?
    };
    let ctx = Arc::new(ctx);
    let mut words = Vec::new();
    for c in 0..chunks {
        let offset = c as f64 * CHUNK_SECS;
        let audio = samples(&video, offset, CHUNK_SECS.min(duration - offset)).await?;
        let ctx = ctx.clone();
        let p = progress.clone();
        let chunk_words = tokio::task::spawn_blocking(move || -> Result<Vec<Word>> {
            let mut state = ctx.create_state()?;
            let mut params =
                whisper_rs::FullParams::new(whisper_rs::SamplingStrategy::Greedy { best_of: 1 });
            params.set_language(Some("en"));
            params.set_token_timestamps(true);
            params.set_split_on_word(true);
            params.set_max_len(1);
            params.set_print_progress(false);
            params.set_print_realtime(false);
            params.set_print_timestamps(false);
            params.set_progress_callback_safe(move |pct: i32| {
                p(
                    "transcribing",
                    c * 100 + pct.clamp(0, 100) as u64,
                    chunks * 100,
                )
            });
            state.full(params, &audio)?;
            let mut out = Vec::new();
            for i in 0..state.full_n_segments() {
                let Some(seg) = state.get_segment(i) else {
                    continue;
                };
                let text = seg.to_str_lossy()?.to_string();
                // Timestamps are in 10 ms units.
                push_word(
                    &mut out,
                    &text,
                    seg.start_timestamp() as f64 / 100.0 + offset,
                    seg.end_timestamp() as f64 / 100.0 + offset,
                );
            }
            Ok(out)
        })
        .await??;
        words.extend(chunk_words);
    }
    let text = words
        .iter()
        .map(|w| w.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let t = Transcript { words, text };
    library::save_transcript(id, &t)?;
    Ok(t)
}

/// Adds one word segment, skipping markers like [BLANK_AUDIO] and (music).
pub fn push_word(out: &mut Vec<Word>, text: &str, start: f64, end: f64) {
    let text = text.trim();
    if text.is_empty() || text.starts_with('[') || text.starts_with('(') {
        return;
    }
    out.push(Word {
        start,
        end: end.max(start),
        text: text.to_string(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_markers_and_keeps_words() {
        let mut w = Vec::new();
        push_word(&mut w, " Hello", 0.0, 0.4);
        push_word(&mut w, " [BLANK_AUDIO]", 0.4, 0.9);
        push_word(&mut w, " (music)", 0.9, 1.0);
        push_word(&mut w, " world", 600.9, 601.3);
        assert_eq!(
            w.iter().map(|x| x.text.as_str()).collect::<Vec<_>>(),
            ["Hello", "world"]
        );
        assert_eq!(w[1].start, 600.9);
    }

    #[test]
    fn model_paths_live_in_app_data() {
        assert!(model_path(Model::BaseEn).ends_with("models/ggml-base.en.bin"));
        assert!(
            model_path(Model::SmallEn)
                .to_string_lossy()
                .contains(&BRAND.identifier)
        );
    }
}
