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
    /// Cameras, including an iPhone used as a Continuity Camera (listed under the phone's name).
    #[serde(default)]
    pub cameras: Vec<Device>,
    /// iPhone and iPad screens connected over USB (shown once the ffmpeg patch allows them).
    #[serde(default)]
    pub phones: Vec<Device>,
}

impl Devices {
    /// Replaces ffmpeg's "Capture screen N" with the display's real name (same order as the
    /// system's display list), or "Display N+1" when the name is unknown.
    pub fn name_screens(&mut self, names: &[String]) {
        for (i, d) in self.screens.iter_mut().enumerate() {
            let n = d
                .name
                .strip_prefix("Capture screen ")
                .and_then(|n| n.trim().parse::<usize>().ok())
                .unwrap_or(i);
            d.name = names
                .get(n)
                .filter(|s| !s.trim().is_empty())
                .cloned()
                .unwrap_or_else(|| format!("Display {}", n + 1));
        }
    }
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
        .context("ffmpeg is missing from this app. Reinstall it (developers: node scripts/fetch-sidecars.mjs)")?;
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
        } else if is_phone_screen(name) {
            d.phones.push(dev);
        } else {
            d.cameras.push(dev);
        }
    }
    d
}

/// A USB iPhone or iPad screen is listed under the device's name ("Ana's iPhone"); its Continuity
/// Camera ends in "Camera", and Desk View in "Desk View Camera".
fn is_phone_screen(name: &str) -> bool {
    let n = name.to_lowercase();
    (n.contains("iphone") || n.contains("ipad")) && !n.contains("camera")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartOptions {
    pub screen: String,
    /// None records without sound.
    pub mic: Option<String>,
    /// A phone screen recorded next to the laptop screen, as its own file.
    #[serde(default)]
    pub phone: Option<PhoneSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PhoneSource {
    /// An iPhone or iPad over USB: an AVFoundation device index.
    Ios {
        index: String,
        name: String,
        #[serde(default)]
        layout: Option<crate::phone::PhonePanel>,
    },
    /// An Android phone through scrcpy: the adb serial.
    Android {
        serial: String,
        name: String,
        #[serde(default)]
        layout: Option<crate::phone::PhonePanel>,
    },
}

impl PhoneSource {
    pub fn name(&self) -> &str {
        match self {
            PhoneSource::Ios { name, .. } | PhoneSource::Android { name, .. } => name,
        }
    }
    pub fn layout(&self) -> crate::phone::PhonePanel {
        match self {
            PhoneSource::Ios { layout, .. } | PhoneSource::Android { layout, .. } => {
                layout.clone().unwrap_or_default()
            }
        }
    }
}

/// When a process started delivering frames: host clock seconds (AVFoundation timestamps; the same
/// clock for every process on this Mac) and wall-clock ms.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Started {
    pub host: Option<f64>,
    pub wall_ms: u64,
}
type StartCell = Arc<Mutex<Option<Started>>>;

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// ffmpeg prints "  Duration: N/A, start: 105623.123456, bitrate: N/A" once the input delivers.
pub fn parse_start(line: &str) -> Option<f64> {
    let rest = line.split("start: ").nth(1)?;
    rest.split(',').next()?.trim().parse().ok()
}

/// Offset of a phone part against its screen segment, in seconds: exact on the shared host clock
/// when both are AVFoundation (USB iPhone), else from wall-clock times.
pub fn part_offset(screen: Started, phone: Started) -> f64 {
    match (screen.host, phone.host) {
        (Some(a), Some(b)) => b - a,
        _ => (phone.wall_ms as f64 - screen.wall_ms as f64) / 1000.0,
    }
}

struct PhoneRec {
    source: PhoneSource,
    parts: Vec<(PathBuf, StartCell)>,
    child: Option<Child>,
}

struct Session {
    id: String,
    opts: StartOptions,
    segments: Vec<PathBuf>,
    starts: Vec<StartCell>,
    phone: Option<PhoneRec>,
    child: Option<Child>,
    log: Arc<Mutex<Vec<String>>>,
    elapsed_before: f64,
    resumed_at: Option<Instant>,
}

#[derive(Default)]
pub struct Capture {
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
        .context("ffmpeg is missing from this app. Reinstall it (developers: node scripts/fetch-sidecars.mjs)")?;
    let log = s.log.clone();
    let started: StartCell = Default::default();
    let cell = started.clone();
    if let Some(err) = child.stderr.take() {
        tokio::spawn(async move {
            let mut lines = BufReader::new(err).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                if let Some(host) = parse_start(&l) {
                    cell.lock().unwrap().get_or_insert(Started {
                        host: Some(host),
                        wall_ms: now_ms(),
                    });
                }
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
    s.starts.push(started);
    s.child = Some(child);
    s.resumed_at = Some(Instant::now());
    if s.phone.is_some() {
        spawn_phone(s).await?;
    }
    Ok(())
}

/// Records the phone screen for the current segment, as its own file.
async fn spawn_phone(s: &mut Session) -> Result<()> {
    let id = s.id.clone();
    let Some(p) = s.phone.as_mut() else {
        return Ok(());
    };
    let file = library::dir(&id).join(format!("tracks/phone-{}.mp4", p.parts.len()));
    std::fs::create_dir_all(file.parent().expect("tracks folder"))?;
    let mut cmd = match &p.source {
        PhoneSource::Ios { index, .. } => {
            let mut c = Command::new(media::tool("ffmpeg"));
            c.args([
                "-hide_banner",
                "-y",
                "-f",
                "avfoundation",
                "-framerate",
                "30",
                "-i",
            ])
            .arg(format!("{index}:none"))
            .args(["-c:v", "h264_videotoolbox", "-b:v", "8M", "-realtime", "1"])
            .arg(&file)
            .stdin(Stdio::piped());
            c
        }
        PhoneSource::Android { serial, .. } => {
            let adb = crate::android::adb_path().context(
                "Android tools are not installed. Open Connect Phone and choose Get Android Tools.",
            )?;
            let mut c = Command::new(media::tool("scrcpy"));
            c.args(crate::android::record_args(serial, &file))
                .env("ADB", adb)
                .env("SCRCPY_SERVER_PATH", crate::android::scrcpy_server())
                .stdin(Stdio::null());
            c
        }
    };
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .context("could not start the phone recording")?;
    let started: StartCell = Default::default();
    // ffmpeg reports the host-clock start; scrcpy says when recording begins.
    for stream in [
        child
            .stdout
            .take()
            .map(|o| Box::new(o) as Box<dyn tokio::io::AsyncRead + Unpin + Send>),
        child
            .stderr
            .take()
            .map(|e| Box::new(e) as Box<dyn tokio::io::AsyncRead + Unpin + Send>),
    ]
    .into_iter()
    .flatten()
    {
        let cell = started.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stream).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                let host = parse_start(&l);
                if host.is_some() || l.contains("Recording started") {
                    cell.lock().unwrap().get_or_insert(Started {
                        host,
                        wall_ms: now_ms(),
                    });
                }
            }
        });
    }
    p.parts.push((file, started));
    p.child = Some(child);
    Ok(())
}

async fn stop_phone(p: &mut PhoneRec) {
    if let Some(mut child) = p.child.take() {
        match p.source {
            PhoneSource::Ios { .. } => {
                if let Some(mut stdin) = child.stdin.take() {
                    let _ = stdin.write_all(b"q").await;
                }
            }
            // scrcpy finalizes the file on SIGINT, like Ctrl+C.
            PhoneSource::Android { .. } => {
                if let Some(pid) = child.id() {
                    let _ = Command::new("/bin/kill")
                        .args(["-INT", &pid.to_string()])
                        .status()
                        .await;
                }
            }
        }
        if tokio::time::timeout(std::time::Duration::from_secs(10), child.wait())
            .await
            .is_err()
        {
            let _ = child.kill().await;
        }
    }
}

async fn stop_child(s: &mut Session) {
    if let Some(p) = s.phone.as_mut() {
        stop_phone(p).await;
    }
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

impl Capture {
    pub async fn start(&self, opts: StartOptions) -> Result<String> {
        let mut g = self.session.lock().await;
        if g.is_some() {
            bail!("already recording");
        }
        let id = library::new_id();
        std::fs::create_dir_all(library::dir(&id))?;
        let mut s = Session {
            id: id.clone(),
            phone: opts.phone.clone().map(|source| PhoneRec {
                source,
                parts: vec![],
                child: None,
            }),
            opts,
            segments: vec![],
            starts: vec![],
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
        // Screen segment lengths, before they are joined.
        let mut lens = vec![];
        for p in &parts {
            lens.push(media::duration(p).await.unwrap_or(0.0));
        }
        let mut v = finish(&s.id, &parts).await?;
        let first = s.starts.first().and_then(|c| *c.lock().unwrap());
        v.rec_started_ms = first.map(|f| f.wall_ms).unwrap_or_else(now_ms);
        v.rec_duration = v.duration;
        if let Some(p) = &s.phone {
            let mut pieces = vec![];
            for (k, (file, cell)) in p.parts.iter().enumerate() {
                let (Some(screen), Some(len)) = (
                    s.starts.get(k).and_then(|c| *c.lock().unwrap()),
                    lens.get(k),
                ) else {
                    continue;
                };
                if !library::exists(file) {
                    continue;
                }
                let phone_start = cell.lock().unwrap().unwrap_or(Started {
                    host: None,
                    wall_ms: screen.wall_ms,
                });
                pieces.push((file.clone(), part_offset(screen, phone_start), *len));
            }
            if !pieces.is_empty() {
                let out = crate::phone::track_path(&s.id, crate::phone::TrackKind::PhoneScreen);
                crate::phone::normalize_parts(&pieces, &out).await?;
                for (f, _, _) in &pieces {
                    let _ = std::fs::remove_file(f);
                }
                v.tracks.push(crate::phone::Track {
                    kind: crate::phone::TrackKind::PhoneScreen,
                    device: p.source.name().to_string(),
                    offset: pieces[0].1,
                });
                v.phone = p.source.layout();
            }
        }
        library::save(&v)?;
        crate::phone::render(&v.id).await?;
        Ok(v)
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
    // Voice level for new recordings (Settings). The raw audio stays in video.mp4.
    let default = crate::ai::load_settings().voice_default.clamped();
    if !default.is_off() {
        let mut v = v;
        v.voice = default;
        library::save(&v)?;
        crate::voice::render(id, &default).await?;
        return Ok(v);
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phones_cameras_and_start_times() {
        assert!(is_phone_screen("Ana\u{2019}s iPhone"));
        assert!(!is_phone_screen("Ana\u{2019}s iPhone Camera"));
        assert!(!is_phone_screen("Ana\u{2019}s iPhone Desk View Camera"));
        assert_eq!(
            parse_start("  Duration: N/A, start: 105623.123456, bitrate: N/A"),
            Some(105623.123456)
        );
        assert_eq!(parse_start("frame=  10 fps=30"), None);
        // Same host clock: exact. Otherwise wall clock.
        let screen = Started {
            host: Some(100.0),
            wall_ms: 5_000,
        };
        assert!(
            (part_offset(
                screen,
                Started {
                    host: Some(100.35),
                    wall_ms: 9_999
                }
            ) - 0.35)
                .abs()
                < 1e-9
        );
        assert_eq!(
            part_offset(
                screen,
                Started {
                    host: None,
                    wall_ms: 5_200
                }
            ),
            0.2
        );
    }

    #[test]
    fn parses_avfoundation_device_list() {
        let log = "[AVFoundation indev @ 0x1] AVFoundation video devices:\n\
                   [AVFoundation indev @ 0x1] [0] FaceTime HD Camera\n\
                   [AVFoundation indev @ 0x1] [1] Capture screen 0\n\
                   [AVFoundation indev @ 0x1] [2] Capture screen 1\n\
                   [AVFoundation indev @ 0x1] AVFoundation audio devices:\n\
                   [AVFoundation indev @ 0x1] [0] MacBook Pro Microphone\n";
        let d = super::parse_devices(log);
        let mut named = d.clone();
        named.name_screens(&["Built-in Retina Display".to_string()]);
        assert_eq!(named.screens[0].name, "Built-in Retina Display");
        assert_eq!(named.screens[1].name, "Display 2");
        assert_eq!(d.screens.len(), 2);
        assert_eq!(d.cameras[0].name, "FaceTime HD Camera");
        assert_eq!(d.screens[1].index, "2");
        assert_eq!(d.mics[0].name, "MacBook Pro Microphone");
    }
}
