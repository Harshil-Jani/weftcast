//! ffmpeg / ffprobe helpers: probing, thumbnails, trimming, cut rendering, caption burn-in.
use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::{Context, Result, bail};

use crate::cuts::Range;

/// ffmpeg from PATH, Homebrew or MacPorts. GUI apps on macOS do not inherit the shell PATH.
pub fn tool(name: &str) -> PathBuf {
    for dir in ["/opt/homebrew/bin", "/usr/local/bin", "/opt/local/bin"] {
        let p = Path::new(dir).join(name);
        if p.exists() {
            return p;
        }
    }
    PathBuf::from(name)
}

pub async fn run(name: &str, args: &[&str]) -> Result<String> {
    let out = tokio::process::Command::new(tool(name))
        .args(args)
        .stdin(Stdio::null())
        .output()
        .await
        .with_context(|| format!("{name} is not installed (see `npm run doctor`)"))?;
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    if !out.status.success() {
        let tail: Vec<&str> = stderr.lines().rev().take(6).collect();
        bail!(
            "{name} failed: {}",
            tail.into_iter().rev().collect::<Vec<_>>().join("\n")
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string() + &stderr)
}

pub async fn duration(p: &Path) -> Result<f64> {
    let out = run(
        "ffprobe",
        &[
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=nw=1:nk=1",
            &p.to_string_lossy(),
        ],
    )
    .await?;
    out.lines()
        .next()
        .and_then(|l| l.trim().parse().ok())
        .context("could not read video duration")
}

pub async fn thumbnail(video: &Path, out: &Path, at: f64) -> Result<()> {
    let at = format!("{at:.2}");
    run(
        "ffmpeg",
        &[
            "-y",
            "-ss",
            &at,
            "-i",
            &video.to_string_lossy(),
            "-frames:v",
            "1",
            "-vf",
            "scale=640:-2",
            &out.to_string_lossy(),
        ],
    )
    .await?;
    Ok(())
}

pub async fn silences(video: &Path, noise_db: i32, min: f64) -> Result<String> {
    let filter = format!("silencedetect=noise={noise_db}dB:d={min}");
    run(
        "ffmpeg",
        &[
            "-hide_banner",
            "-nostats",
            "-i",
            &video.to_string_lossy(),
            "-af",
            &filter,
            "-vn",
            "-f",
            "null",
            "-",
        ],
    )
    .await
}

pub async fn extract_wav(video: &Path, out: &Path) -> Result<()> {
    run(
        "ffmpeg",
        &[
            "-y",
            "-i",
            &video.to_string_lossy(),
            "-vn",
            "-ac",
            "1",
            "-ar",
            "16000",
            "-c:a",
            "pcm_s16le",
            &out.to_string_lossy(),
        ],
    )
    .await?;
    Ok(())
}

const ENCODE: &[&str] = &[
    "-c:v",
    "h264_videotoolbox",
    "-b:v",
    "8M",
    "-c:a",
    "aac",
    "-b:a",
    "160k",
    "-movflags",
    "+faststart",
];

pub async fn trim(input: &Path, out: &Path, start: f64, end: f64) -> Result<()> {
    let (s, e) = (format!("{start:.3}"), format!("{end:.3}"));
    let mut args = vec![
        "-y",
        "-i",
        input.to_str().unwrap_or_default(),
        "-ss",
        &s,
        "-to",
        &e,
    ];
    args.extend_from_slice(ENCODE);
    let o = out.to_string_lossy();
    args.push(&o);
    run("ffmpeg", &args).await?;
    Ok(())
}

/// Keeps only `keep` ranges, joined back to back.
pub async fn render_keep(input: &Path, out: &Path, keep: &[Range]) -> Result<()> {
    anyhow::ensure!(!keep.is_empty(), "nothing would be left of the video");
    let expr = keep
        .iter()
        .map(|r| format!("between(t,{:.3},{:.3})", r.start, r.end))
        .collect::<Vec<_>>()
        .join("+");
    let vf = format!("select='{expr}',setpts=N/FRAME_RATE/TB");
    let af = format!("aselect='{expr}',asetpts=N/SR/TB");
    let mut args = vec![
        "-y",
        "-i",
        input.to_str().unwrap_or_default(),
        "-vf",
        &vf,
        "-af",
        &af,
    ];
    args.extend_from_slice(ENCODE);
    let o = out.to_string_lossy();
    args.push(&o);
    run("ffmpeg", &args).await?;
    Ok(())
}

pub async fn can_burn() -> bool {
    run("ffmpeg", &["-hide_banner", "-filters"])
        .await
        .map(|o| o.contains(" subtitles "))
        .unwrap_or(false)
}

/// Adds the captions as a selectable track (players show a CC button). Works with any ffmpeg.
pub async fn embed_captions(input: &Path, srt: &Path, out: &Path) -> Result<()> {
    run(
        "ffmpeg",
        &[
            "-y",
            "-i",
            &input.to_string_lossy(),
            "-i",
            &srt.to_string_lossy(),
            "-map",
            "0",
            "-map",
            "1",
            "-c",
            "copy",
            "-c:s",
            "mov_text",
            "-metadata:s:s:0",
            "language=eng",
            &out.to_string_lossy(),
        ],
    )
    .await?;
    Ok(())
}

/// Draws the captions into the picture. Needs ffmpeg built with libass.
pub async fn burn_captions(input: &Path, srt: &Path, out: &Path) -> Result<()> {
    let style = "FontName=Helvetica,FontSize=18,PrimaryColour=&H00FFFFFF,OutlineColour=&H80000000,BorderStyle=3,Outline=1,Shadow=0,MarginV=28";
    let srt_escaped = srt
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace(':', "\\:")
        .replace('\'', "\\'");
    let vf = format!("subtitles='{srt_escaped}':force_style='{style}'");
    let mut args = vec!["-y", "-i", input.to_str().unwrap_or_default(), "-vf", &vf];
    args.extend_from_slice(ENCODE);
    let o = out.to_string_lossy();
    args.push(&o);
    run("ffmpeg", &args).await?;
    Ok(())
}

pub async fn concat(parts: &[PathBuf], out: &Path) -> Result<()> {
    if parts.len() == 1 {
        tokio::fs::rename(&parts[0], out).await?;
        return Ok(());
    }
    let list = out.with_extension("txt");
    let body: String = parts
        .iter()
        .map(|p| format!("file '{}'\n", p.to_string_lossy().replace('\'', "'\\''")))
        .collect();
    tokio::fs::write(&list, body).await?;
    run(
        "ffmpeg",
        &[
            "-y",
            "-f",
            "concat",
            "-safe",
            "0",
            "-i",
            &list.to_string_lossy(),
            "-c",
            "copy",
            "-movflags",
            "+faststart",
            &out.to_string_lossy(),
        ],
    )
    .await?;
    let _ = tokio::fs::remove_file(&list).await;
    for p in parts {
        let _ = tokio::fs::remove_file(p).await;
    }
    Ok(())
}
