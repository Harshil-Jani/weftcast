//! Screen + mic capture through an ffmpeg process (avfoundation on macOS).
//! Pause/resume closes the current segment and starts a new one; stop joins the segments.
//! Windows (gdigrab/ddagrab + dshow) and Linux (x11grab/kmsgrab + pulse) plug in at `input_args`.
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};

use crate::library::{self, Video};
use crate::media;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub index: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Devices {
    pub screens: Vec<Device>,
    pub mics: Vec<Device>,
}

pub async fn devices() -> Result<Devices> {
    let out = Command::new(media::tool("ffmpeg"))
        .args([
            "-hide_banner",
            "-f",
            "avfoundation",
            "-list_devices",
            "true",
            "-i",
            "",
        ])
        .output()
        .await
        .context("ffmpeg is not installed (see `npm run doctor`)")?;
    Ok(parse_devices(&String::from_utf8_lossy(&out.stderr)))
}

pub fn parse_devices(log: &str) -> Devices {
    let mut d = Devices::default();
    let mut audio = false;
    for line in log.lines() {
        if line.contains("AVFoundation audio devices") {
            audio = true;
            continue;
        }
        let Some(rest) = line.split("] [").nth(1) else {
            continue;
        };
        let Some((idx, name)) = rest.split_once("] ") else {
            continue;
        };
        let dev = Device {
            index: idx.to_string(),
            name: name.trim().to_string(),
        };
        if audio {
            d.mics.push(dev);
        } else if name.starts_with("Capture screen") {
            d.screens.push(dev);
        }
    }
    d
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartOptions {
    pub screen: String,
    /// None records without sound.
    pub mic: Option<String>,
}

struct Session {
    id: String,
    opts: StartOptions,
    segments: Vec<PathBuf>,
    child: Option<Child>,
    log: Arc<Mutex<Vec<String>>>,
    elapsed_before: f64,
    resumed_at: Option<Instant>,
}

#[derive(Default)]
pub struct Recorder {
    session: tokio::sync::Mutex<Option<Session>>,
}

#[derive(Debug, Serialize)]
pub struct RecState {
    pub recording: bool,
    pub paused: bool,
    pub elapsed: f64,
}

fn input_args(o: &StartOptions) -> Vec<String> {
    let audio = o.mic.clone().unwrap_or_else(|| "none".into());
    vec![
        "-f".into(),
        "avfoundation".into(),
        "-capture_cursor".into(),
        "1".into(),
        "-framerate".into(),
        "30".into(),
        "-pixel_format".into(),
        "nv12".into(),
        "-i".into(),
        format!("{}:{}", o.screen, audio),
    ]
}

async fn spawn_segment(s: &mut Session) -> Result<()> {
    let seg = library::dir(&s.id).join(format!("segment-{}.mp4", s.segments.len()));
    let mut args = vec!["-hide_banner".to_string(), "-y".into()];
    args.extend(input_args(&s.opts));
    args.extend(
        [
            "-vf",
            "scale=trunc(iw/2)*2:trunc(ih/2)*2",
            "-c:v",
            "h264_videotoolbox",
            "-b:v",
            "10M",
            "-realtime",
            "1",
            "-c:a",
            "aac",
            "-b:a",
            "160k",
        ]
        .map(String::from),
    );
    args.push(seg.to_string_lossy().to_string());

    let mut child = Command::new(media::tool("ffmpeg"))
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .context("ffmpeg is not installed (see `npm run doctor`)")?;
    let log = s.log.clone();
    if let Some(err) = child.stderr.take() {
        tokio::spawn(async move {
            let mut lines = BufReader::new(err).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                let mut g = log.lock().unwrap();
                g.push(l);
                if g.len() > 40 {
                    g.remove(0);
                }
            }
        });
    }
    // A missing permission makes ffmpeg exit right away; report it instead of recording nothing.
    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
    if let Some(status) = child.try_wait()? {
        let tail = s.log.lock().unwrap().join("\n");
        bail!(
            "Recording could not start (ffmpeg exited with {status}). On macOS, allow Screen Recording and Microphone \
             for this app in System Settings > Privacy & Security, then try again.\n\n{tail}"
        );
    }
    s.segments.push(seg);
    s.child = Some(child);
    s.resumed_at = Some(Instant::now());
    Ok(())
}

async fn stop_child(s: &mut Session) {
    if let Some(mut child) = s.child.take() {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(b"q").await;
        }
        if tokio::time::timeout(std::time::Duration::from_secs(10), child.wait())
            .await
            .is_err()
        {
            let _ = child.kill().await;
        }
    }
    if let Some(t) = s.resumed_at.take() {
        s.elapsed_before += t.elapsed().as_secs_f64();
    }
}

impl Recorder {
    pub async fn start(&self, opts: StartOptions) -> Result<String> {
        let mut g = self.session.lock().await;
        if g.is_some() {
            bail!("already recording");
        }
        let id = library::new_id();
        std::fs::create_dir_all(library::dir(&id))?;
        let mut s = Session {
            id: id.clone(),
            opts,
            segments: vec![],
            child: None,
            log: Default::default(),
            elapsed_before: 0.0,
            resumed_at: None,
        };
        if let Err(e) = spawn_segment(&mut s).await {
            let _ = std::fs::remove_dir_all(library::dir(&id));
            return Err(e);
        }
        *g = Some(s);
        Ok(id)
    }

    pub async fn pause(&self) -> Result<()> {
        let mut g = self.session.lock().await;
        let s = g.as_mut().context("not recording")?;
        stop_child(s).await;
        Ok(())
    }

    pub async fn resume(&self) -> Result<()> {
        let mut g = self.session.lock().await;
        let s = g.as_mut().context("not recording")?;
        if s.child.is_none() {
            spawn_segment(s).await?;
        }
        Ok(())
    }

    pub async fn state(&self) -> RecState {
        let g = self.session.lock().await;
        match g.as_ref() {
            None => RecState {
                recording: false,
                paused: false,
                elapsed: 0.0,
            },
            Some(s) => RecState {
                recording: true,
                paused: s.child.is_none(),
                elapsed: s.elapsed_before
                    + s.resumed_at
                        .map(|t| t.elapsed().as_secs_f64())
                        .unwrap_or(0.0),
            },
        }
    }

    pub async fn cancel(&self) -> Result<()> {
        let mut g = self.session.lock().await;
        if let Some(mut s) = g.take() {
            stop_child(&mut s).await;
            let _ = std::fs::remove_dir_all(library::dir(&s.id));
        }
        Ok(())
    }

    /// Stops, joins the segments and adds the recording to the library.
    pub async fn stop(&self) -> Result<Video> {
        let mut s = self.session.lock().await.take().context("not recording")?;
        stop_child(&mut s).await;
        let parts: Vec<PathBuf> = s
            .segments
            .iter()
            .filter(|p| library::exists(p))
            .cloned()
            .collect();
        if parts.is_empty() {
            let _ = std::fs::remove_dir_all(library::dir(&s.id));
            bail!("nothing was recorded");
        }
        finish(&s.id, &parts).await
    }
}

/// Turns finished segment files into a library entry (also used to import a file).
pub async fn finish(id: &str, parts: &[PathBuf]) -> Result<Video> {
    let video = library::video_path(id);
    media::concat(parts, &video).await?;
    let duration = media::duration(&video).await?;
    let _ = media::thumbnail(&video, &library::thumb_path(id), (duration * 0.1).min(2.0)).await;
    let created = chrono::Local::now();
    let v = Video {
        id: id.to_string(),
        share_id: library::new_share_id(),
        title: format!("Recording {}", created.format("%b %-d, %H:%M")),
        created_at: created.to_rfc3339(),
        duration,
        ..Default::default()
    };
    library::save(&v)?;
    Ok(v)
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_avfoundation_device_list() {
        let log = "[AVFoundation indev @ 0x1] AVFoundation video devices:\n\
                   [AVFoundation indev @ 0x1] [0] FaceTime HD Camera\n\
                   [AVFoundation indev @ 0x1] [1] Capture screen 0\n\
                   [AVFoundation indev @ 0x1] [2] Capture screen 1\n\
                   [AVFoundation indev @ 0x1] AVFoundation audio devices:\n\
                   [AVFoundation indev @ 0x1] [0] MacBook Pro Microphone\n";
        let d = super::parse_devices(log);
        assert_eq!(d.screens.len(), 2);
        assert_eq!(d.screens[1].index, "2");
        assert_eq!(d.mics[0].name, "MacBook Pro Microphone");
    }
}
