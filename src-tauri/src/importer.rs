//! Bringing outside videos into the library: a local file, or a YouTube link through the bundled
//! yt-dlp. Only for videos the user owns or has the rights to; the source link is kept in metadata.
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::capture;
use crate::library::{self, Video};
use crate::media;

pub async fn file(path: &Path) -> Result<Video> {
    ensure!(path.is_file(), "no file at {}", path.display());
    let id = library::new_id();
    std::fs::create_dir_all(library::dir(&id))?;
    let seg = library::dir(&id).join("segment-0.mp4");
    // Re-wrap into MP4 (and convert audio to AAC) without re-encoding video when possible.
    let res = media::run(
        "ffmpeg",
        &[
            "-y",
            "-i",
            &path.to_string_lossy(),
            "-map",
            "0:v:0",
            "-map",
            "0:a:0?",
            "-c:v",
            "copy",
            "-c:a",
            "aac",
            "-b:a",
            "160k",
            "-movflags",
            "+faststart",
            &seg.to_string_lossy(),
        ],
    )
    .await;
    if res.is_err() {
        media::run(
            "ffmpeg",
            &[
                "-y",
                "-i",
                &path.to_string_lossy(),
                "-c:v",
                "h264_videotoolbox",
                "-b:v",
                "8M",
                "-c:a",
                "aac",
                "-b:a",
                "160k",
                &seg.to_string_lossy(),
            ],
        )
        .await?;
    }
    let mut v = capture::finish(&id, &[seg]).await?;
    v.title = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or(v.title);
    library::save(&v)?;
    Ok(v)
}

pub fn is_youtube(url: &str) -> bool {
    let u = url.trim();
    let Some(rest) = u
        .strip_prefix("https://")
        .or_else(|| u.strip_prefix("http://"))
    else {
        return false;
    };
    let host = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .to_lowercase();
    host == "youtu.be" || host == "youtube.com" || host.ends_with(".youtube.com")
}

/// Downloads with the bundled yt-dlp (up to 1080p MP4). `progress` gets 0 to 100.
pub async fn youtube(url: &str, progress: impl Fn(u64)) -> Result<Video> {
    ensure!(is_youtube(url), "that does not look like a YouTube link");
    ensure!(
        media::available("yt-dlp"),
        "the YouTube importer is missing from this app"
    );
    let id = library::new_id();
    std::fs::create_dir_all(library::dir(&id))?;
    let out: PathBuf = library::dir(&id).join("source.%(ext)s");
    let mut child = tokio::process::Command::new(media::tool("yt-dlp"))
        .args([
            "--no-playlist",
            "--newline",
            "--no-warnings",
            "-f",
            "bv*[height<=1080][ext=mp4]+ba[ext=m4a]/b[ext=mp4]/b",
            "--merge-output-format",
            "mp4",
            "--ffmpeg-location",
        ])
        .arg(media::tool("ffmpeg"))
        .arg("-o")
        .arg(&out)
        .args([
            "--print",
            "after_move:filepath",
            "--print",
            "before_dl:title",
            url,
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .context("could not start the YouTube importer")?;
    let mut lines = BufReader::new(child.stdout.take().context("stdout")?).lines();
    let (mut title, mut file) = (None, None);
    while let Some(l) = lines.next_line().await? {
        if let Some(p) = l
            .split("[download]")
            .nth(1)
            .and_then(|r| r.trim().split('%').next())
            .and_then(|n| n.trim().parse::<f64>().ok())
        {
            progress(p as u64);
        } else if l.starts_with('/') && l.ends_with(".mp4") {
            file = Some(PathBuf::from(l));
        } else if !l.starts_with('[') && title.is_none() && !l.trim().is_empty() {
            title = Some(l.trim().to_string());
        }
    }
    let status = child.wait().await?;
    let file = file.filter(|_| status.success()).context(
        "the download failed. If it keeps failing, use Update YouTube importer in Settings.",
    )?;
    let seg = library::dir(&id).join("segment-0.mp4");
    std::fs::rename(&file, &seg)?;
    let mut v = capture::finish(&id, &[seg]).await?;
    if let Some(t) = title {
        v.title = t;
    }
    v.source_url = Some(url.trim().to_string());
    library::save(&v)?;
    Ok(v)
}

#[cfg(test)]
mod tests {
    #[test]
    fn recognises_youtube_links_only() {
        assert!(super::is_youtube(
            "https://www.youtube.com/watch?v=NYYmafpjtUQ"
        ));
        assert!(super::is_youtube("https://youtu.be/NYYmafpjtUQ"));
        assert!(!super::is_youtube("file:///etc/passwd"));
        assert!(!super::is_youtube("https://example.com/youtube.com/x"));
    }
}
