//! Phone tracks: the phone's screen (USB) and phone cameras (Wi-Fi), recorded as separate files and
//! composited on render, so the layout can change after recording.
//!
//! Timelines:
//! - Each track is first *normalized* to the recording timeline: shifted by its start offset against
//!   the screen (padded with black or trimmed), held on its last frame, and cut to the recording length.
//! - The video keeps a `timeline` of segments mapping its current time back to recording time
//!   (every cut, trim or speed change appends to it), and the same segments are applied to each track.
//!
//! Everything that builds an ffmpeg command is a pure function with unit tests.
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use crate::cuts::Segment;
use crate::library::{self, Video};
use crate::media;

/// Where the phone screen sits next to the laptop screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Layout {
    #[default]
    Right,
    Left,
    Pip,
    Split,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Size {
    S,
    #[default]
    M,
    L,
}

/// A range (current timeline) where the phone panel is shown with a given layout.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LayoutRange {
    pub start: f64,
    pub end: f64,
    pub layout: Layout,
}

/// The phone-screen panel. With no `ranges` it shows for the whole video in `layout`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PhonePanel {
    pub enabled: bool,
    pub layout: Layout,
    pub size: Size,
    /// Picture-in-picture position: the panel's centre as fractions of the frame.
    pub pip: (f64, f64),
    pub ranges: Vec<LayoutRange>,
}

impl Default for PhonePanel {
    fn default() -> Self {
        Self {
            enabled: true,
            layout: Layout::Right,
            size: Size::M,
            pip: (0.86, 0.72),
            ranges: vec![],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AngleMode {
    /// Only shown where B-roll ranges cut it in.
    #[default]
    Off,
    Pip,
    Split,
}

/// A second camera angle (hands, a whiteboard, a product): PiP, split, or B-roll over ranges.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AnglePanel {
    pub mode: AngleMode,
    pub broll: Vec<BRoll>,
}

/// Full-frame cut-in of a track over a range (current timeline).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BRoll {
    pub start: f64,
    pub end: f64,
    pub track: TrackKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "snake_case")]
pub enum TrackKind {
    /// The phone's screen (iPhone over USB, Android through scrcpy).
    PhoneScreen,
    /// A phone camera used as the face cam (shown in the bubble while recording).
    PhoneCamera,
    /// A phone camera used as a second angle.
    Angle,
}

impl TrackKind {
    pub fn file(self) -> &'static str {
        match self {
            TrackKind::PhoneScreen => "tracks/phone-screen.mp4",
            TrackKind::PhoneCamera => "tracks/phone-camera.mp4",
            TrackKind::Angle => "tracks/angle.mp4",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            TrackKind::PhoneScreen => "Phone screen",
            TrackKind::PhoneCamera => "Phone camera",
            TrackKind::Angle => "Second angle",
        }
    }
}

/// A finished, normalized track (already on the recording timeline).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub kind: TrackKind,
    /// Phone or device name, for display.
    #[serde(default)]
    pub device: String,
    /// Seconds the track started after the screen (negative: before). Kept for the record.
    #[serde(default)]
    pub offset: f64,
}

/// Composes two edits: `old` maps current time to recording time, `new` maps the next current time
/// to the old current time. The result maps the next current time straight to recording time.
pub fn compose_timeline(old: &[Segment], new: &[Segment], duration: f64) -> Vec<Segment> {
    let base: Vec<Segment> = if old.is_empty() {
        vec![Segment {
            start: 0.0,
            end: duration,
            speed: 1.0,
        }]
    } else {
        old.to_vec()
    };
    let mut out = Vec::new();
    for n in new {
        // Walk the old segments in output order; `at` is where each starts in the old current timeline.
        let mut at = 0.0;
        for o in &base {
            let len = o.out_len();
            let (a, b) = (n.start.max(at), n.end.min(at + len));
            if b - a > 1e-6 {
                out.push(Segment {
                    start: o.start + (a - at) * o.speed,
                    end: o.start + (b - at) * o.speed,
                    speed: o.speed * n.speed,
                });
            }
            at += len;
        }
    }
    out
}

/// Keeps a video's tracks in step with a timeline edit (`segs` are in the video's current time):
/// extends the timeline and moves phone and B-roll ranges to the new times. A range whose start was
/// cut moves to where the next kept part begins; one that is cut entirely is dropped.
pub fn follow_edit(v: &mut Video, segs: &[Segment], old_duration: f64) {
    v.timeline = compose_timeline(&v.timeline, segs, old_duration);
    let remap = |a: f64, b: f64| -> Option<(f64, f64)> {
        let mut out: Option<(f64, f64)> = None;
        let mut at = 0.0;
        for s in segs {
            let (x, y) = (a.max(s.start), b.min(s.end));
            if y > x {
                let (ma, mb) = (at + (x - s.start) / s.speed, at + (y - s.start) / s.speed);
                out = Some(out.map_or((ma, mb), |(p, _)| (p, mb)));
            }
            at += s.out_len();
        }
        out.filter(|(x, y)| y - x > 0.05)
    };
    v.phone.ranges = v
        .phone
        .ranges
        .iter()
        .filter_map(|r| {
            remap(r.start, r.end).map(|(start, end)| LayoutRange {
                start,
                end,
                layout: r.layout,
            })
        })
        .collect();
    v.angle.broll = v
        .angle
        .broll
        .iter()
        .filter_map(|r| {
            remap(r.start, r.end).map(|(start, end)| BRoll {
                start,
                end,
                track: r.track,
            })
        })
        .collect();
}

/// ffmpeg video filter that puts a track on the recording timeline: `offset` seconds after the
/// screen started (negative: before), then exactly `duration` long, 30 fps, even dimensions.
pub fn normalize_filter(offset: f64, duration: f64) -> String {
    let lead = if offset >= 0.0 {
        format!("tpad=start_duration={offset:.3}:color=black,")
    } else {
        format!("trim=start={:.3},setpts=PTS-STARTPTS,", -offset)
    };
    format!(
        "{lead}fps=30,scale=trunc(iw/2)*2:trunc(ih/2)*2,tpad=stop_mode=clone:stop_duration={duration:.3},trim=duration={duration:.3},setpts=PTS-STARTPTS"
    )
}

/// Cuts one video-only input the way the main video was cut (`timeline` is recording time per segment).
fn timeline_chain(input: &str, timeline: &[Segment], label: &str) -> String {
    if timeline.is_empty() {
        return format!("{input}setpts=PTS-STARTPTS[{label}];");
    }
    let mut g = String::new();
    let n = timeline.len();
    if n > 1 {
        g += &format!(
            "{input}split={n}{};",
            (0..n).map(|i| format!("[{label}s{i}]")).collect::<String>()
        );
    }
    for (i, s) in timeline.iter().enumerate() {
        let src = if n > 1 {
            format!("[{label}s{i}]")
        } else {
            input.to_string()
        };
        g += &format!(
            "{src}trim=start={:.3}:end={:.3},setpts=(PTS-STARTPTS)/{}[{label}t{i}];",
            s.start, s.end, s.speed
        );
    }
    let joined: String = (0..n).map(|i| format!("[{label}t{i}]")).collect();
    g + &format!("{joined}concat=n={n}:v=1:a=0[{label}];")
}

fn size_frac(s: Size, layout: Layout) -> f64 {
    match (layout, s) {
        (Layout::Pip, Size::S) => 0.36,
        (Layout::Pip, Size::M) => 0.46,
        (Layout::Pip, Size::L) => 0.6,
        (_, Size::S) => 0.66,
        (_, Size::M) => 0.8,
        (_, Size::L) => 0.92,
    }
}

/// Pixel geometry of the device frame for a phone `ph` px tall with aspect `ar` (w/h).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    /// Outer size of the frame (bezel included).
    pub w: i64,
    pub h: i64,
    /// Bezel width; the screen sits inside it.
    pub bezel: i64,
    pub radius: i64,
}

pub fn frame_for(screen_h: i64, ar: f64) -> Frame {
    let sh = even(screen_h as f64);
    let sw = even(sh as f64 * ar);
    let bezel = even((sw as f64 * 0.045).max(8.0));
    Frame {
        w: sw + 2 * bezel,
        h: sh + 2 * bezel,
        bezel,
        radius: even(sw as f64 * 0.16),
    }
}

fn even(x: f64) -> i64 {
    ((x / 2.0).round() as i64 * 2).max(2)
}

/// geq expression for a rounded rectangle mask (255 inside) of size W x H, inset by `inset`, corner `r`.
fn rounded(inset: i64, r: i64) -> String {
    let (x, y) = (format!("(X-{inset})"), format!("(Y-{inset})"));
    let (w, h) = (format!("(W-{})", 2 * inset), format!("(H-{})", 2 * inset));
    format!(
        "lte(hypot(max(max({r}-{x},{x}-({w}-1-{r})),0),max(max({r}-{y},{y}-({h}-1-{r})),0)),{r})"
    )
}

/// One-frame ffmpeg source for the device frame: a near-black rounded bezel with a thin lighter rim
/// (so it reads on dark and light pictures) and a rounded transparent window. No device artwork.
pub fn frame_source(f: Frame) -> String {
    let outer = rounded(0, f.radius);
    let rim = format!("(1-{})", rounded(2, (f.radius - 2).max(4)));
    let inner = rounded(f.bezel, (f.radius - f.bezel).max(4));
    let c = format!("'10+52*{rim}'");
    format!(
        "color=c=black:s={}x{}:r=1:d=1,format=rgba,geq=r={c}:g={c}:b={c}:a='255*{outer}*(1-{inner})'",
        f.w, f.h
    )
}

/// What to composite. Input 0 is the main video (with audio), then each present track.
#[derive(Debug, Clone)]
pub struct Plan {
    pub w: i64,
    pub h: i64,
    pub duration: f64,
    pub timeline: Vec<Segment>,
    /// The phone screen: (input index, aspect w/h, panel settings).
    pub phone: Option<(usize, f64, PhonePanel)>,
    /// A second angle shown as PiP or split: (input index, mode).
    pub angle: Option<(usize, AngleMode)>,
    /// Full-frame cut-ins and the input index of each track they use.
    pub broll: Vec<BRoll>,
    pub broll_inputs: Vec<(TrackKind, usize)>,
}

fn enable(ranges: &[(f64, f64)]) -> String {
    ranges
        .iter()
        .map(|(a, b)| format!("between(t,{a:.3},{b:.3})"))
        .collect::<Vec<_>>()
        .join("+")
}

/// Where the main picture and the phone frame go for one layout: (main box x, y, w, h), frame x, y.
fn place(
    layout: Layout,
    w: i64,
    h: i64,
    fr: Frame,
    pip: (f64, f64),
) -> ((i64, i64, i64, i64), i64, i64) {
    let m = even(h as f64 * 0.04);
    let fy = (h - fr.h) / 2;
    match layout {
        Layout::Right => ((m, m, w - fr.w - 4 * m, h - 2 * m), w - fr.w - 2 * m, fy),
        Layout::Left => ((fr.w + 3 * m, m, w - fr.w - 4 * m, h - 2 * m), 2 * m, fy),
        Layout::Split => (
            (m, m, w / 2 - 2 * m, h - 2 * m),
            w / 2 + (w / 2 - fr.w) / 2,
            fy,
        ),
        Layout::Pip => (
            (0, 0, w, h),
            ((w as f64 * pip.0) as i64 - fr.w / 2).clamp(m, w - fr.w - m),
            ((h as f64 * pip.1) as i64 - fr.h / 2).clamp(m, h - fr.h - m),
        ),
    }
}

/// The full filter graph: [vout] is the composited picture. Audio is input 0's, untouched, so lip
/// sync with the main recording stays exactly what it was.
pub fn compose_filter(p: &Plan) -> String {
    let (w, h) = (p.w, p.h);
    let mut g = String::new();
    // Phone ranges grouped by layout; each layout used gets its own branch of the main picture.
    let mut groups: Vec<(Layout, Vec<(f64, f64)>)> = vec![];
    if let Some((_, _, panel)) = &p.phone
        && panel.enabled
    {
        if panel.ranges.is_empty() {
            groups.push((panel.layout, vec![(0.0, p.duration + 1.0)]));
        }
        for r in &panel.ranges {
            match groups.iter_mut().find(|(l, _)| *l == r.layout) {
                Some((_, v)) => v.push((r.start, r.end)),
                None => groups.push((r.layout, vec![(r.start, r.end)])),
            }
        }
    }
    let n = groups.len();
    g += &format!(
        "[0:v]setsar=1,split={}{};",
        n + 1,
        (0..=n).map(|i| format!("[m{i}]")).collect::<String>()
    );
    g += &format!(
        "[m0]scale={w}:{h}:force_original_aspect_ratio=decrease,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2:color=0x1c1c1e[base];"
    );
    let mut cur = "base".to_string();

    if let Some((pi, ar, panel)) = &p.phone
        && n > 0
    {
        // The phone track, cut like the main video, then one copy per layout.
        g += &timeline_chain(&format!("[{pi}:v]"), &p.timeline, "ph");
        g += &format!(
            "[ph]split={n}{};",
            (0..n).map(|i| format!("[ph{i}]")).collect::<String>()
        );
        for (i, (layout, ranges)) in groups.iter().enumerate() {
            let k = i + 1;
            let fr = frame_for((h as f64 * size_frac(panel.size, *layout)) as i64, *ar);
            let (sw, sh) = (fr.w - 2 * fr.bezel, fr.h - 2 * fr.bezel);
            let ((bx, by, bw, bh), fx, fy) = place(*layout, w, h, fr, panel.pip);
            let (bw, bh) = (even(bw as f64), even(bh as f64));
            g += &format!(
                "[m{k}]scale={bw}:{bh}:force_original_aspect_ratio=decrease[mm{k}];\
                 color=c=0x1c1c1e:s={w}x{h}:r=30:d={d:.3}[bg{k}];\
                 [bg{k}][mm{k}]overlay=x={bx}+({bw}-w)/2:y={by}+({bh}-h)/2:shortest=1[lay{k}];\
                 [ph{i}]scale={sw}:{sh}:force_original_aspect_ratio=increase,crop={sw}:{sh}[ps{k}];\
                 [lay{k}][ps{k}]overlay=x={x}:y={y}[lp{k}];\
                 {frame},loop=loop=-1:size=1,setpts=N/30/TB[frl{k}];\
                 [lp{k}][frl{k}]overlay=x={fx}:y={fy}:shortest=1[lf{k}];\
                 [{cur}][lf{k}]overlay=enable='{e}'[c{k}];",
                d = p.duration + 1.0,
                frame = frame_source(fr),
                x = fx + fr.bezel,
                y = fy + fr.bezel,
                e = enable(ranges),
            );
            cur = format!("c{k}");
        }
    }

    if let Some((ai, mode)) = &p.angle {
        g += &timeline_chain(&format!("[{ai}:v]"), &p.timeline, "ang");
        match mode {
            AngleMode::Pip | AngleMode::Off => {
                let (ah, m) = (even(h as f64 * 0.3), even(h as f64 * 0.04));
                g += &format!("[ang]scale=-2:{ah}[angs];[{cur}][angs]overlay=x={m}:y=H-h-{m}[ca];");
            }
            AngleMode::Split => {
                let half = w / 2;
                g += &format!(
                    "[{cur}]crop={half}:{h}:(iw-{half})/2:0[cl];\
                     [ang]scale={half}:{h}:force_original_aspect_ratio=increase,crop={half}:{h}[cr];\
                     [cl][cr]hstack=inputs=2[ca];"
                );
            }
        }
        cur = "ca".into();
    }

    for (kind, idx) in &p.broll_inputs {
        let ranges: Vec<(f64, f64)> = p
            .broll
            .iter()
            .filter(|b| b.track == *kind)
            .map(|b| (b.start, b.end))
            .collect();
        if ranges.is_empty() {
            continue;
        }
        let l = format!("br{idx}");
        g += &timeline_chain(&format!("[{idx}:v]"), &p.timeline, &l);
        g += &format!(
            "[{l}]scale={w}:{h}:force_original_aspect_ratio=increase,crop={w}:{h}[{l}f];[{cur}][{l}f]overlay=enable='{}'[c{l}];",
            enable(&ranges)
        );
        cur = format!("c{l}");
    }
    g + &format!("[{cur}]format=yuv420p[vout]")
}

pub fn has_tracks(v: &Video) -> bool {
    !v.tracks.is_empty()
}

pub fn track_path(id: &str, kind: TrackKind) -> PathBuf {
    library::dir(id).join(kind.file())
}

/// Puts a raw track file on the recording timeline (see `normalize_filter`).
pub async fn normalize(raw: &Path, out: &Path, offset: f64, duration: f64) -> Result<()> {
    if let Some(d) = out.parent() {
        tokio::fs::create_dir_all(d).await?;
    }
    let vf = normalize_filter(offset, duration);
    media::run(
        "ffmpeg",
        &[
            "-y",
            "-i",
            &raw.to_string_lossy(),
            "-an",
            "-vf",
            &vf,
            "-c:v",
            "h264_videotoolbox",
            "-b:v",
            "8M",
            "-movflags",
            "+faststart",
            &out.to_string_lossy(),
        ],
    )
    .await?;
    Ok(())
}

/// Joins per-segment phone recordings (one per pause/resume part), each placed against its
/// screen segment by its own offset, into one normalized track.
pub async fn normalize_parts(parts: &[(PathBuf, f64, f64)], out: &Path) -> Result<()> {
    ensure!(!parts.is_empty(), "no phone recording");
    let dir = out.parent().context("track folder")?.to_path_buf();
    tokio::fs::create_dir_all(&dir).await?;
    let mut done = vec![];
    for (i, (raw, offset, dur)) in parts.iter().enumerate() {
        let p = dir.join(format!("part-{i}.mp4"));
        normalize(raw, &p, *offset, *dur).await?;
        done.push(p);
    }
    if done.len() == 1 {
        tokio::fs::rename(&done[0], out).await?;
    } else {
        media::concat(&done, out).await?;
        for p in done {
            let _ = tokio::fs::remove_file(p).await;
        }
    }
    Ok(())
}

async fn probe_size(p: &Path) -> Result<(i64, i64)> {
    let out = media::run(
        "ffprobe",
        &[
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height",
            "-of",
            "csv=p=0:s=x",
            &p.to_string_lossy(),
        ],
    )
    .await?;
    let (a, b) = out.trim().split_once('x').context("no video stream")?;
    Ok((a.trim().parse()?, b.trim().parse()?))
}

pub fn composed_path(id: &str) -> PathBuf {
    library::dir(id).join("composed.mp4")
}

/// Renders composed.mp4 from the current video and its tracks, or removes it when nothing is laid out.
pub async fn render(id: &str) -> Result<()> {
    let v = library::load(id)?;
    let out = composed_path(id);
    let phone = v
        .tracks
        .iter()
        .find(|t| t.kind == TrackKind::PhoneScreen)
        .filter(|_| v.phone.enabled);
    let angle_on = v.tracks.iter().any(|t| t.kind == TrackKind::Angle)
        && (v.angle.mode != AngleMode::Off || !v.angle.broll.is_empty());
    let broll_kinds: Vec<TrackKind> = v
        .angle
        .broll
        .iter()
        .map(|b| b.track)
        .filter(|k| v.tracks.iter().any(|t| t.kind == *k))
        .collect();
    if phone.is_none() && !angle_on && broll_kinds.is_empty() {
        let _ = tokio::fs::remove_file(&out).await;
        return Ok(());
    }
    // The main picture with the current voice level applied.
    let main = library::voiced_or_video(id);
    let (w, h) = probe_size(&main).await?;
    let (w, h) = (even(w as f64), even(h as f64));
    let mut inputs: Vec<String> = vec!["-i".into(), main.to_string_lossy().into()];
    let mut next = 1;
    let mut plan = Plan {
        w,
        h,
        duration: v.duration,
        timeline: v.timeline.clone(),
        phone: None,
        angle: None,
        broll: vec![],
        broll_inputs: vec![],
    };
    if phone.is_some() {
        let tp = track_path(id, TrackKind::PhoneScreen);
        let (pw, ph) = probe_size(&tp).await?;
        let ar = pw as f64 / ph as f64;
        inputs.extend(["-i".into(), tp.to_string_lossy().into()]);
        plan.phone = Some((next, ar, v.phone.clone()));
        next += 1;
    }
    if angle_on && v.angle.mode != AngleMode::Off {
        inputs.extend([
            "-i".into(),
            track_path(id, TrackKind::Angle).to_string_lossy().into(),
        ]);
        plan.angle = Some((next, v.angle.mode));
        next += 1;
    }
    plan.broll = v.angle.broll.clone();
    for k in broll_kinds {
        if plan.broll_inputs.iter().any(|(x, _)| *x == k) {
            continue;
        }
        inputs.extend(["-i".into(), track_path(id, k).to_string_lossy().into()]);
        plan.broll_inputs.push((k, next));
        next += 1;
    }
    let graph = compose_filter(&plan);
    let tmp = library::dir(id).join("composed.tmp.mp4");
    let mut args: Vec<String> = vec!["-y".into()];
    args.extend(inputs);
    args.extend([
        "-filter_complex".into(),
        graph,
        "-map".into(),
        "[vout]".into(),
        "-map".into(),
        "0:a?".into(),
        "-c:v".into(),
        "h264_videotoolbox".into(),
        "-b:v".into(),
        "10M".into(),
        "-c:a".into(),
        "copy".into(),
        "-t".into(),
        format!("{:.3}", v.duration),
        "-movflags".into(),
        "+faststart".into(),
        tmp.to_string_lossy().into(),
    ]);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    media::run("ffmpeg", &refs).await?;
    tokio::fs::rename(&tmp, &out).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(start: f64, end: f64, speed: f64) -> Segment {
        Segment { start, end, speed }
    }

    #[test]
    fn timelines_compose() {
        // First edit: cut 10..20 out of a 60s recording.
        let first = vec![seg(0.0, 10.0, 1.0), seg(20.0, 60.0, 1.0)];
        let t1 = compose_timeline(&[], &first, 60.0);
        assert_eq!(t1, first);
        // Second edit (in the 50s edited video): keep 5..15, i.e. recording 5..10 and 20..25.
        let t2 = compose_timeline(&t1, &[seg(5.0, 15.0, 1.0)], 50.0);
        assert_eq!(t2, vec![seg(5.0, 10.0, 1.0), seg(20.0, 25.0, 1.0)]);
        // Speeding up its first 2s (2x): segment times are source times, so that is recording 5..7 at 2x.
        let t3 = compose_timeline(&t2, &[seg(0.0, 2.0, 2.0), seg(2.0, 10.0, 1.0)], 10.0);
        assert_eq!(
            t3,
            vec![
                seg(5.0, 7.0, 2.0),
                seg(7.0, 10.0, 1.0),
                seg(20.0, 25.0, 1.0)
            ]
        );
    }

    #[test]
    fn edits_move_ranges_and_extend_the_timeline() {
        let mut v = Video {
            duration: 30.0,
            ..Default::default()
        };
        v.phone.ranges = vec![
            LayoutRange {
                start: 2.0,
                end: 6.0,
                layout: Layout::Pip,
            },
            LayoutRange {
                start: 11.0,
                end: 13.0,
                layout: Layout::Right,
            },
            LayoutRange {
                start: 22.0,
                end: 26.0,
                layout: Layout::Left,
            },
        ];
        v.angle.broll = vec![BRoll {
            start: 8.0,
            end: 12.0,
            track: TrackKind::Angle,
        }];
        // Cut 10..20 out.
        follow_edit(&mut v, &[seg(0.0, 10.0, 1.0), seg(20.0, 30.0, 1.0)], 30.0);
        assert_eq!(v.timeline, vec![seg(0.0, 10.0, 1.0), seg(20.0, 30.0, 1.0)]);
        let r: Vec<(f64, f64)> = v.phone.ranges.iter().map(|r| (r.start, r.end)).collect();
        assert_eq!(r, vec![(2.0, 6.0), (12.0, 16.0)], "11..13 was cut entirely");
        assert_eq!(
            (v.angle.broll[0].start, v.angle.broll[0].end),
            (8.0, 10.0),
            "clipped to what is left"
        );
    }

    #[test]
    fn offsets_normalize() {
        // Phone started 0.4s after the screen: pad 0.4s of black in front.
        let f = normalize_filter(0.4, 12.0);
        assert!(f.starts_with("tpad=start_duration=0.400:color=black,"));
        assert!(f.ends_with("trim=duration=12.000,setpts=PTS-STARTPTS"));
        // Phone started 0.25s before: drop its first 0.25s.
        assert!(normalize_filter(-0.25, 12.0).starts_with("trim=start=0.250,setpts=PTS-STARTPTS,"));
    }

    #[test]
    fn frame_geometry_is_even_and_rounded() {
        let f = frame_for(864, 1170.0 / 2532.0);
        assert_eq!(f.h, 864 + 2 * f.bezel);
        assert!(f.w % 2 == 0 && f.h % 2 == 0 && f.bezel % 2 == 0);
        assert!(f.radius > f.bezel);
        let src = frame_source(f);
        assert!(src.contains(&format!("s={}x{}", f.w, f.h)) && src.contains("hypot"));
        assert!(!src.to_lowercase().contains("apple"));
    }

    fn plan(layout: Layout) -> Plan {
        Plan {
            w: 1920,
            h: 1080,
            duration: 20.0,
            timeline: vec![],
            phone: Some((
                1,
                1170.0 / 2532.0,
                PhonePanel {
                    layout,
                    ..Default::default()
                },
            )),
            angle: None,
            broll: vec![],
            broll_inputs: vec![],
        }
    }

    #[test]
    fn layouts_build_distinct_graphs() {
        let right = compose_filter(&plan(Layout::Right));
        let left = compose_filter(&plan(Layout::Left));
        let pip = compose_filter(&plan(Layout::Pip));
        let split = compose_filter(&plan(Layout::Split));
        for g in [&right, &left, &pip, &split] {
            assert!(g.ends_with("format=yuv420p[vout]"), "{g}");
            assert!(
                g.contains("geq=") && g.contains("loop=loop=-1:size=1"),
                "{g}"
            );
            assert!(!g.contains("[0:a]"), "audio must pass through untouched");
        }
        // Right: frame near the right edge; Left: near the left edge.
        let fx = |g: &str| -> i64 {
            let i = g.find("[frl1]overlay=x=").unwrap() + "[frl1]overlay=x=".len();
            g[i..].split(':').next().unwrap().parse().unwrap()
        };
        assert!(
            fx(&right) > 1300 && fx(&left) < 200,
            "{} {}",
            fx(&right),
            fx(&left)
        );
        // Split: the main picture fits the left half; PiP keeps it full frame.
        assert!(split.contains("[m1]scale=872:"), "{split}");
        assert!(pip.contains("[m1]scale=1920:1080"), "{pip}");
    }

    #[test]
    fn ranges_and_timeline_apply_to_tracks() {
        let mut p = plan(Layout::Right);
        p.timeline = vec![seg(0.0, 5.0, 1.0), seg(8.0, 12.0, 2.0)];
        if let Some((_, _, panel)) = &mut p.phone {
            panel.ranges = vec![
                LayoutRange {
                    start: 1.0,
                    end: 3.0,
                    layout: Layout::Right,
                },
                LayoutRange {
                    start: 4.0,
                    end: 6.0,
                    layout: Layout::Pip,
                },
            ];
        }
        let g = compose_filter(&p);
        assert!(
            g.contains("trim=start=8.000:end=12.000,setpts=(PTS-STARTPTS)/2"),
            "{g}"
        );
        assert!(g.contains("concat=n=2:v=1:a=0[ph]"), "{g}");
        assert!(g.contains("enable='between(t,1.000,3.000)'"), "{g}");
        assert!(g.contains("enable='between(t,4.000,6.000)'"), "{g}");
        assert!(g.contains("split=3[m0][m1][m2]"), "{g}");
        // Each layout gets a frame sized for it (PiP is smaller than the side panel).
        assert_eq!(g.matches("geq=").count(), 2, "{g}");
    }

    #[test]
    fn angle_modes_and_broll() {
        let mut p = plan(Layout::Right);
        p.phone = None;
        p.angle = Some((1, AngleMode::Pip));
        p.broll = vec![BRoll {
            start: 2.0,
            end: 4.0,
            track: TrackKind::Angle,
        }];
        p.broll_inputs = vec![(TrackKind::Angle, 2)];
        let g = compose_filter(&p);
        assert!(g.contains("[angs]overlay=x="), "{g}");
        assert!(
            g.contains("[2:v]setpts=PTS-STARTPTS[br2]")
                && g.contains("enable='between(t,2.000,4.000)'"),
            "{g}"
        );
        p.angle = Some((1, AngleMode::Split));
        p.broll_inputs = vec![];
        assert!(compose_filter(&p).contains("hstack=inputs=2"));
    }
}
