//! End to end for phone layouts, with synthetic media and no real phone:
//! `RECORDER_LIBRARY=/tmp/lib OUT=/tmp/shots cargo run --example phone_e2e`
//!
//! A "laptop" recording (testsrc 1920x1080, a one-frame white flash and a beep at 3.0s) and a
//! "phone" recording (testsrc2 1170x2532 portrait, a flash at its own 2.0s) that started 0.5s after
//! the screen. For each layout it composites, probes the output, saves a frame, and checks:
//! - the phone flash lands at 2.5s (offset sync), the screen flash at 3.0s;
//! - the beep and the screen flash are within one frame (A/V sync);
//!
//! Then it cuts the first second through the real edit path and checks both flashes move with it.
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use app_lib::phone::{self, AngleMode, BRoll, Layout, Track, TrackKind};
use app_lib::{capture, edits, library, media};

const FRAME: f64 = 1.0 / 30.0;

async fn ff(args: &[&str]) -> Result<String> {
    media::run("ffmpeg", args).await
}

/// Times where the mean brightness jumps (flash frames).
async fn flashes(file: &Path) -> Result<Vec<f64>> {
    let log = media::run(
        "ffmpeg",
        &[
            "-hide_banner",
            "-i",
            &file.to_string_lossy(),
            "-vf",
            "signalstats,metadata=print:key=lavfi.signalstats.YAVG",
            "-an",
            "-f",
            "null",
            "-",
        ],
    )
    .await?;
    let mut out = vec![];
    let (mut t, mut prev) = (0.0, None::<f64>);
    for l in log.lines() {
        if let Some(p) = l.split("pts_time:").nth(1) {
            t = p.trim().parse().unwrap_or(t);
        }
        if let Some(v) = l.split("YAVG=").nth(1) {
            let y: f64 = v.trim().parse().unwrap_or(0.0);
            if let Some(p) = prev
                && y - p > 4.0
            {
                out.push(t);
            }
            prev = Some(y);
        }
    }
    Ok(out)
}

/// When the first beep starts (end of the leading silence).
async fn beep(file: &Path) -> Result<f64> {
    let log = media::run(
        "ffmpeg",
        &[
            "-hide_banner",
            "-i",
            &file.to_string_lossy(),
            "-af",
            "silencedetect=n=-40dB:d=0.5",
            "-vn",
            "-f",
            "null",
            "-",
        ],
    )
    .await?;
    log.lines()
        .find_map(|l| {
            l.split("silence_end: ")
                .nth(1)?
                .split_whitespace()
                .next()?
                .parse()
                .ok()
        })
        .context("no beep found")
}

fn near(list: &[f64], want: f64) -> Option<f64> {
    list.iter()
        .copied()
        .min_by(|a, b| (a - want).abs().total_cmp(&(b - want).abs()))
        .filter(|t| (t - want).abs() <= FRAME + 1e-3)
}

#[tokio::main]
async fn main() -> Result<()> {
    let out_dir = PathBuf::from(std::env::var("OUT").unwrap_or_else(|_| "/tmp/phone-e2e".into()));
    std::fs::create_dir_all(&out_dir)?;
    let work = library::root().join("_inputs");
    std::fs::create_dir_all(&work)?;
    let laptop = work.join("laptop.mp4");
    let phone_raw = work.join("phone.mp4");
    let angle_raw = work.join("angle.mp4");

    // Laptop: one white frame at 3.0s, a 1 kHz beep from 3.0s, silence elsewhere.
    ff(&[
        "-y",
        "-f",
        "lavfi",
        "-i",
        "testsrc=s=1920x1080:r=30:d=12",
        "-f",
        "lavfi",
        "-i",
        "sine=f=1000:r=48000:d=12",
        "-vf",
        "drawbox=c=white:t=fill:enable='eq(n,90)'",
        "-af",
        "volume=enable='not(between(t,3,3.4))':volume=0",
        "-c:v",
        "h264_videotoolbox",
        "-b:v",
        "8M",
        "-c:a",
        "aac",
        "-shortest",
        &laptop.to_string_lossy(),
    ])
    .await?;
    // Phone: portrait, white frame at its own 2.0s.
    ff(&[
        "-y",
        "-f",
        "lavfi",
        "-i",
        "testsrc2=s=1170x2532:r=30:d=11.5",
        "-vf",
        "drawbox=c=white:t=fill:enable='eq(n,60)'",
        "-c:v",
        "h264_videotoolbox",
        "-b:v",
        "8M",
        &phone_raw.to_string_lossy(),
    ])
    .await?;
    // Second angle: landscape, for PiP, split and B-roll.
    ff(&[
        "-y",
        "-f",
        "lavfi",
        "-i",
        "smptehdbars=s=1280x720:r=30:d=12",
        "-c:v",
        "h264_videotoolbox",
        "-b:v",
        "4M",
        &angle_raw.to_string_lossy(),
    ])
    .await?;

    let id = library::new_id();
    std::fs::create_dir_all(library::dir(&id))?;
    let mut v = capture::finish(&id, std::slice::from_ref(&laptop)).await?;
    v.title = "Phone layouts (synthetic)".into();
    v.rec_duration = v.duration;
    let offset = 0.5;
    phone::normalize(
        &phone_raw,
        &phone::track_path(&id, TrackKind::PhoneScreen),
        offset,
        v.duration,
    )
    .await?;
    phone::normalize(
        &angle_raw,
        &phone::track_path(&id, TrackKind::Angle),
        0.0,
        v.duration,
    )
    .await?;
    v.tracks = vec![
        Track {
            kind: TrackKind::PhoneScreen,
            device: "Test phone".into(),
            offset,
        },
        Track {
            kind: TrackKind::Angle,
            device: "Test camera".into(),
            offset: 0.0,
        },
    ];
    library::save(&v)?;

    let composed = phone::composed_path(&id);
    let mut report = vec![];
    let cases: Vec<(&str, Layout, AngleMode, Vec<BRoll>)> = vec![
        ("right", Layout::Right, AngleMode::Off, vec![]),
        ("left", Layout::Left, AngleMode::Off, vec![]),
        ("pip", Layout::Pip, AngleMode::Off, vec![]),
        ("split", Layout::Split, AngleMode::Off, vec![]),
        ("angle-pip", Layout::Right, AngleMode::Pip, vec![]),
        ("angle-split", Layout::Right, AngleMode::Split, vec![]),
        (
            "broll",
            Layout::Right,
            AngleMode::Off,
            vec![BRoll {
                start: 6.0,
                end: 8.0,
                track: TrackKind::Angle,
            }],
        ),
    ];
    for (name, layout, mode, broll) in cases {
        let mut v = library::load(&id)?;
        v.phone.enabled = !name.starts_with("angle");
        v.phone.layout = layout;
        v.angle.mode = mode;
        v.angle.broll = broll.clone();
        library::save(&v)?;
        phone::render(&id).await?;
        let probe = media::run(
            "ffprobe",
            &[
                "-v",
                "error",
                "-show_entries",
                "stream=codec_type,width,height:format=duration",
                "-of",
                "compact=p=0:nk=1",
                &composed.to_string_lossy(),
            ],
        )
        .await?;
        let shot_at = if broll.is_empty() { "5" } else { "7" };
        let png = out_dir.join(format!("phone-layout-{name}.png"));
        ff(&[
            "-y",
            "-ss",
            shot_at,
            "-i",
            &composed.to_string_lossy(),
            "-frames:v",
            "1",
            &png.to_string_lossy(),
        ])
        .await?;
        let f = flashes(&composed).await?;
        let b = beep(&composed).await?;
        let screen_flash =
            near(&f, 3.0).with_context(|| format!("{name}: screen flash not at 3.0s: {f:?}"))?;
        ensure!(
            (screen_flash - b).abs() <= FRAME + 1e-3,
            "{name}: A/V off by {:.3}s",
            screen_flash - b
        );
        let phone_flash = if v.phone.enabled {
            Some(
                near(&f, 2.0 + offset)
                    .with_context(|| format!("{name}: phone flash not at 2.5s: {f:?}"))?,
            )
        } else {
            None
        };
        report.push(format!(
            "{name:12} ok  {}  screen flash {screen_flash:.3}s  beep {b:.3}s  A/V {:+.3}s  phone flash {}",
            probe.lines().collect::<Vec<_>>().join(" | "),
            screen_flash - b,
            phone_flash.map(|t| format!("{t:.3}s (offset {offset}s)")).unwrap_or_else(|| "n/a".into())
        ));
    }

    // Cut the first second through the edit path: tracks must follow.
    let mut v = library::load(&id)?;
    v.phone.enabled = true;
    v.phone.layout = Layout::Right;
    v.angle = Default::default();
    library::save(&v)?;
    let plan = edits::validate(
        &serde_json::json!({"ops": [{"op": "cut", "start": 0, "end": 1}, {"op": "show_phone", "start": 2, "end": 9, "layout": "pip"}]}),
        v.duration,
    )?;
    edits::apply(&id, &plan).await?;
    let f = flashes(&composed).await?;
    let after_phone = near(&f, 1.5)
        .with_context(|| format!("after the cut the phone flash should be at 1.5s: {f:?}"))?;
    let after_screen = near(&f, 2.0)
        .with_context(|| format!("after the cut the screen flash should be at 2.0s: {f:?}"))?;
    let b = beep(&composed).await?;
    ensure!(
        (after_screen - b).abs() <= FRAME + 1e-3,
        "after the cut A/V off by {:.3}s",
        after_screen - b
    );
    let png = out_dir.join("phone-layout-ranges-after-cut.png");
    ff(&[
        "-y",
        "-ss",
        "3",
        "-i",
        &composed.to_string_lossy(),
        "-frames:v",
        "1",
        &png.to_string_lossy(),
    ])
    .await?;
    let v = library::load(&id)?;
    report.push(format!(
        "after cut   ok  timeline {:?}  phone range {:?}  phone flash {after_phone:.3}s  screen flash {after_screen:.3}s  beep {b:.3}s",
        v.timeline.iter().map(|s| (s.start, s.end)).collect::<Vec<_>>(),
        v.phone.ranges.iter().map(|r| (r.start, r.end)).collect::<Vec<_>>()
    ));
    println!("{}", report.join("\n"));
    Ok(())
}
