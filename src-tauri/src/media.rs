//! ffmpeg / ffprobe helpers: probing, thumbnails, trimming, cut rendering, caption burn-in.
use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::{Context, Result, bail};

use crate::brand::BRAND;
use crate::cuts::{Range, Segment};

/// Finds a bundled helper (ffmpeg, ffprobe, cloudflared, yt-dlp), in this order:
/// 1. a developer override, e.g. RECORDER_FFMPEG=/path (prefix = brand slug, upper case)
/// 2. for yt-dlp, a newer verified copy fetched by "Update YouTube importer"
/// 3. the sidecar bundled next to the app's executable
/// 4. dev builds only: src-tauri/binaries/<name>-<target>, then PATH
pub fn tool(name: &str) -> PathBuf {
    let key = format!("{}_{}", BRAND.slug, name)
        .to_uppercase()
        .replace('-', "_");
    if let Some(p) = std::env::var_os(&key) {
        return PathBuf::from(p);
    }
    if name == "yt-dlp" {
        let updated = crate::sidecars::updates_dir().join("yt-dlp");
        if updated.exists() {
            return updated;
        }
    }
    if let Some(p) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join(name)))
        .filter(|p| p.exists())
    {
        return p;
    }
    #[cfg(debug_assertions)]
    {
        let dev = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("binaries")
            .join(format!("{name}-{}", env!("TARGET_TRIPLE")));
        if dev.exists() {
            return dev;
        }
        if let Some(p) = on_path(name) {
            return p;
        }
    }
    PathBuf::from(name)
}

pub fn on_path(name: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|d| d.join(name))
        .find(|p| p.is_file())
}

pub fn available(name: &str) -> bool {
    tool(name).is_file()
}

pub async fn run(name: &str, args: &[&str]) -> Result<String> {
    let out = tokio::process::Command::new(tool(name))
        .args(args)
        .stdin(Stdio::null())
        .output()
        .await
        .with_context(|| format!("{name} is missing from this app. Reinstall it (developers: node scripts/fetch-sidecars.mjs)"))?;
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
    let graph = keep_filter(keep, None);
    let mut args = vec![
        "-y",
        "-i",
        input.to_str().unwrap_or_default(),
        "-filter_complex",
        &graph,
        "-map",
        "[v]",
        "-map",
        "[a]",
    ];
    args.extend_from_slice(ENCODE);
    let o = out.to_string_lossy();
    args.push(&o);
    run("ffmpeg", &args).await?;
    Ok(())
}

/// trim/atrim each kept range and concat them, so video and audio are cut identically.
/// With `voice`, the audio filter chain runs once on the joined audio.
pub fn keep_filter(keep: &[Range], voice: Option<&str>) -> String {
    let segs: Vec<Segment> = keep
        .iter()
        .map(|r| Segment {
            start: r.start,
            end: r.end,
            speed: 1.0,
        })
        .collect();
    timeline_filter(&segs, voice)
}

/// The general form: each segment is trimmed, optionally sped up or slowed down, then all are joined.
/// ponytail: one filter chain per segment; fine for hundreds of segments, split into passes if it ever gets huge.
pub fn timeline_filter(segs: &[Segment], voice: Option<&str>) -> String {
    let mut g = String::new();
    for (i, r) in segs.iter().enumerate() {
        let (s, e) = (r.start, r.end);
        if (r.speed - 1.0).abs() < 1e-6 {
            g += &format!(
                "[0:v]trim=start={s:.3}:end={e:.3},setpts=PTS-STARTPTS[v{i}];[0:a]atrim=start={s:.3}:end={e:.3},asetpts=PTS-STARTPTS[a{i}];"
            );
        } else {
            g += &format!(
                "[0:v]trim=start={s:.3}:end={e:.3},setpts=(PTS-STARTPTS)/{f}[v{i}];[0:a]atrim=start={s:.3}:end={e:.3},asetpts=PTS-STARTPTS,{t}[a{i}];",
                f = r.speed,
                t = atempo(r.speed)
            );
        }
    }
    let n = segs.len();
    let inputs: String = (0..n).map(|i| format!("[v{i}][a{i}]")).collect();
    match voice {
        None => g + &format!("{inputs}concat=n={n}:v=1:a=1[v][a]"),
        Some(af) => g + &format!("{inputs}concat=n={n}:v=1:a=1[v][acat];[acat]{af}[a]"),
    }
}

/// atempo is most accurate between 0.5 and 2, so larger changes are chained.
pub fn atempo(speed: f64) -> String {
    let mut left = speed;
    let mut parts = Vec::new();
    while left > 2.0 + 1e-9 {
        parts.push("atempo=2".to_string());
        left /= 2.0;
    }
    while left < 0.5 - 1e-9 {
        parts.push("atempo=0.5".to_string());
        left /= 0.5;
    }
    parts.push(format!("atempo={}", (left * 1000.0).round() / 1000.0));
    parts.join(",")
}

pub async fn render_timeline(input: &Path, out: &Path, segs: &[Segment]) -> Result<()> {
    anyhow::ensure!(!segs.is_empty(), "nothing would be left of the video");
    let graph = timeline_filter(segs, None);
    let mut args = vec![
        "-y",
        "-i",
        input.to_str().unwrap_or_default(),
        "-filter_complex",
        &graph,
        "-map",
        "[v]",
        "-map",
        "[a]",
    ];
    args.extend_from_slice(ENCODE);
    let o = out.to_string_lossy();
    args.push(&o);
    run("ffmpeg", &args).await?;
    Ok(())
}

/// Re-encodes only the audio through `af`; the video stream is copied, so it is fast and lossless.
/// `window` = (start, seconds) for previews.
pub async fn audio_only(
    input: &Path,
    out: &Path,
    af: &str,
    window: Option<(f64, f64)>,
) -> Result<()> {
    let (ss, t) = window
        .map(|(s, d)| (format!("{s:.3}"), format!("{d:.3}")))
        .unwrap_or_default();
    let mut args = vec!["-y"];
    if window.is_some() {
        args.extend(["-ss", &ss, "-t", &t]);
    }
    let i = input.to_string_lossy();
    let o = out.to_string_lossy();
    args.extend([
        "-i",
        &i,
        "-map",
        "0:v:0?",
        "-map",
        "0:a:0",
        "-c:v",
        "copy",
        "-af",
        af,
        "-c:a",
        "aac",
        "-b:a",
        "160k",
        "-movflags",
        "+faststart",
        &o,
    ]);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keep_filter_cuts_video_and_audio_together() {
        let g = keep_filter(
            &[
                Range {
                    start: 0.0,
                    end: 1.5,
                },
                Range {
                    start: 3.0,
                    end: 4.25,
                },
            ],
            None,
        );
        assert!(g.contains("[0:v]trim=start=0.000:end=1.500,setpts=PTS-STARTPTS[v0]"));
        assert!(g.contains("[0:a]atrim=start=3.000:end=4.250,asetpts=PTS-STARTPTS[a1]"));
        assert!(g.ends_with("[v0][a0][v1][a1]concat=n=2:v=1:a=1[v][a]"));
    }

    #[test]
    fn speed_segments_retime_video_and_audio() {
        let g = timeline_filter(
            &[
                Segment {
                    start: 0.0,
                    end: 5.0,
                    speed: 1.0,
                },
                Segment {
                    start: 5.0,
                    end: 25.0,
                    speed: 2.0,
                },
            ],
            None,
        );
        assert!(g.contains("[0:v]trim=start=5.000:end=25.000,setpts=(PTS-STARTPTS)/2[v1]"));
        assert!(g.contains("asetpts=PTS-STARTPTS,atempo=2[a1]"));
    }

    #[test]
    fn atempo_chains_outside_half_to_double() {
        assert_eq!(atempo(1.5), "atempo=1.5");
        assert_eq!(atempo(4.0), "atempo=2,atempo=2");
        assert_eq!(atempo(3.0), "atempo=2,atempo=1.5");
        assert_eq!(atempo(0.25), "atempo=0.5,atempo=0.5");
    }
}
