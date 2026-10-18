//! Exports to the Downloads folder: the video, captions files, or the video with captions.
use std::path::PathBuf;

use anyhow::{Result, bail};

use crate::library::{self, Video};
use crate::media;

fn export_path(v: &Video, ext: &str) -> PathBuf {
    let safe: String = v
        .title
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == ' ' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    dirs::download_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("{}.{ext}", safe.trim()))
}

/// `kind`: mp4, captioned, srt or vtt. Returns the file and an optional note for the user.
pub async fn run(id: &str, kind: &str) -> Result<(PathBuf, Option<String>)> {
    let v = library::load(id)?;
    let words = library::load_transcript(id)
        .map(|t| t.words)
        .unwrap_or_default();
    let src = library::playable_path(id);
    let mut note = None;
    let out = match kind {
        "mp4" => {
            let out = export_path(&v, "mp4");
            tokio::fs::copy(&src, &out).await?;
            out
        }
        "srt" | "vtt" => {
            if words.is_empty() {
                bail!("Transcribe the video first.");
            }
            let out = export_path(&v, kind);
            let body = if kind == "srt" {
                library::to_srt(&words)
            } else {
                library::to_vtt(&words)
            };
            tokio::fs::write(&out, body).await?;
            out
        }
        "captioned" => {
            if words.is_empty() {
                bail!("Transcribe the video first.");
            }
            let srt = library::dir(id).join("captions.srt");
            tokio::fs::write(&srt, library::to_srt(&words)).await?;
            let out = export_path(&v, "mp4");
            if media::can_burn().await {
                media::burn_captions(&src, &srt, &out).await?;
            } else {
                media::embed_captions(&src, &srt, &out).await?;
                note = Some("This ffmpeg can't draw text, so captions were added as a track viewers can turn on (CC).".to_string());
            }
            out
        }
        _ => bail!("unknown export \"{kind}\" (use mp4, captioned, srt or vtt)"),
    };
    Ok((out, note))
}
