//! Shorts: turn one long video into several vertical 9:16 clips. Claude picks self-contained
//! moments from the timed transcript; the user reviews and nudges them; each renders at
//! 1080x1920 with big word-highlighted captions and the video's voice level.
use std::path::PathBuf;

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::ai;
use crate::cuts::{self, Range, Word};
use crate::library::{self, Transcript, Video};
use crate::{media, voice};

/// Short-form yellow; brand accents are often too dark to read on video.
pub const HIGHLIGHT: &str = "#FFD400";
pub const W: u32 = 1080;
pub const H: u32 = 1920;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Clip {
    pub id: String,
    pub start: f64,
    pub end: f64,
    pub title: String,
    pub reason: String,
    pub score: u8,
    pub score_reason: String,
    pub caption: String,
    #[serde(default = "yes")]
    pub selected: bool,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Limits {
    pub count: usize,
    pub min: f64,
    pub max: f64,
    #[serde(default)]
    pub allow_overlap: bool,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            count: 5,
            min: 15.0,
            max: 60.0,
            allow_overlap: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Framing {
    /// Crop the middle of the frame.
    Center,
    /// The whole frame, letterboxed over a blurred, zoomed copy of itself.
    #[default]
    BlurFit,
    /// Screen on top, the camera bubble on the bottom (needs the bubble's position).
    Split,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ShortsFile {
    pub limits: Option<Limits>,
    pub clips: Vec<Clip>,
    #[serde(default)]
    pub rendered: Vec<String>,
}

pub fn dir(id: &str) -> PathBuf {
    library::dir(id).join("shorts")
}

pub fn load(id: &str) -> ShortsFile {
    std::fs::read(dir(id).join("shorts.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save(id: &str, f: &ShortsFile) -> Result<()> {
    std::fs::create_dir_all(dir(id))?;
    std::fs::write(dir(id).join("shorts.json"), serde_json::to_vec_pretty(f)?)?;
    Ok(())
}

/// Snaps a clip to sentences and checks it against the video and the length limits.
pub fn check_clip(c: &mut Clip, sents: &[Range], duration: f64, l: &Limits) -> Result<()> {
    ensure!(
        c.start.is_finite() && c.end.is_finite() && c.end > c.start,
        "clip {} has no length",
        c.id
    );
    let r = cuts::snap_to_sentences(
        Range {
            start: c.start.max(0.0),
            end: c.end.min(duration),
        },
        sents,
    );
    c.start = r.start;
    c.end = r.end;
    ensure!(
        c.start >= 0.0 && c.end <= duration + 0.05,
        "clip {} is outside the video",
        c.id
    );
    let len = c.end - c.start;
    ensure!(
        len >= l.min - 1.0 && len <= l.max + 1.0,
        "clip {} runs {:.0}s, outside {:.0} to {:.0}s",
        c.id,
        len,
        l.min,
        l.max
    );
    ensure!(!c.title.trim().is_empty(), "clip {} has no title", c.id);
    c.score = c.score.clamp(1, 10);
    Ok(())
}

/// Keeps valid clips, best first, dropping overlaps (unless allowed) and anything past `count`.
pub fn validate(
    raw: Vec<Clip>,
    sents: &[Range],
    duration: f64,
    l: &Limits,
) -> (Vec<Clip>, Vec<String>) {
    let mut ok = Vec::new();
    let mut rejected = Vec::new();
    for mut c in raw {
        match check_clip(&mut c, sents, duration, l) {
            Ok(()) => ok.push(c),
            Err(e) => rejected.push(e.to_string()),
        }
    }
    ok.sort_by_key(|c| std::cmp::Reverse(c.score));
    let mut kept: Vec<Clip> = Vec::new();
    for c in ok {
        if kept.len() >= l.count {
            break;
        }
        if !l.allow_overlap
            && let Some(o) = kept.iter().find(|k| c.start < k.end && k.start < c.end)
        {
            rejected.push(format!("clip {} overlaps clip {}", c.id, o.id));
            continue;
        }
        kept.push(c);
    }
    kept.sort_by(|a, b| a.start.total_cmp(&b.start));
    for (i, c) in kept.iter_mut().enumerate() {
        c.id = format!("c{}", i + 1);
    }
    (kept, rejected)
}

fn sentence_lines(t: &Transcript, from: f64, to: f64) -> String {
    cuts::sentences(&t.words)
        .iter()
        .filter(|s| s.start >= from && s.start < to)
        .map(|r| {
            let text: Vec<&str> = t
                .words
                .iter()
                .filter(|w| w.start >= r.start - 1e-6 && w.end <= r.end + 1e-6)
                .map(|w| w.text.as_str())
                .collect();
            format!("[{:.1}-{:.1}] {}", r.start, r.end, text.join(" "))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn clip_schema() -> Value {
    json!({"type": "object", "additionalProperties": false, "required": ["clips"], "properties": {"clips": {"type": "array", "items": {
    "type": "object", "additionalProperties": false,
    "required": ["start", "end", "title", "reason", "score", "score_reason", "caption"],
    "properties": {
        "start": {"type": "number"}, "end": {"type": "number"}, "title": {"type": "string"},
        "reason": {"type": "string"}, "score": {"type": "integer"}, "score_reason": {"type": "string"},
        "caption": {"type": "string"}
    }}}}})
}

const SYSTEM: &str = "You are a short-form video editor. From a long video's transcript, pick moments that work as \
    vertical shorts on their own: a clear hook in the first seconds, one idea, a satisfying end. Start and end on the \
    sentence boundaries given. Titles are punchy hooks (under 60 characters). Score virality 1 to 10 honestly. \
    Post captions are one or two lines with no hashtag spam.";

/// Long transcripts are split into windows so each request stays focused; the best clips win.
const WINDOW_SECS: f64 = 1800.0;

pub async fn suggest(
    v: &Video,
    t: &Transcript,
    l: &Limits,
    avoid: &[Range],
) -> Result<(Vec<Clip>, Vec<String>)> {
    ensure!(!t.words.is_empty(), "the transcript is empty");
    ensure!(
        l.min >= 5.0 && l.max >= l.min && l.count >= 1 && l.count <= 20,
        "check the clip count and lengths"
    );
    let windows = (v.duration / WINDOW_SECS).ceil().max(1.0) as usize;
    let per_window = (l.count * 2).div_ceil(windows).max(2);
    let mut raw = Vec::new();
    for w in 0..windows {
        let (from, to) = (
            w as f64 * WINDOW_SECS,
            ((w + 1) as f64 * WINDOW_SECS).min(v.duration),
        );
        let lines = sentence_lines(t, from, to);
        if lines.is_empty() {
            continue;
        }
        let avoid_txt = if avoid.is_empty() {
            String::new()
        } else {
            format!(
                "\nDo not reuse these ranges: {}",
                avoid
                    .iter()
                    .map(|r| format!("{:.1}-{:.1}", r.start, r.end))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        let user = format!(
            "Video: \"{}\" ({:.0}s). Pick up to {per_window} clips, each {:.0} to {:.0} seconds{}.{avoid_txt}\n\nSentences [start-end] in seconds:\n{lines}",
            v.title,
            v.duration,
            l.min,
            l.max,
            if l.allow_overlap {
                ""
            } else {
                ", not overlapping"
            }
        );
        let out = ai::ask_json(SYSTEM, &user, &clip_schema()).await?;
        for (i, c) in out["clips"].as_array().into_iter().flatten().enumerate() {
            let mut c = c.clone();
            c["id"] = json!(format!("w{w}-{i}"));
            if let Ok(clip) = serde_json::from_value::<Clip>(c) {
                raw.push(clip);
            }
        }
    }
    let sents = cuts::sentences(&t.words);
    let raw: Vec<Clip> = raw
        .into_iter()
        .filter(|c| !avoid.iter().any(|a| c.start < a.end && a.start < c.end))
        .collect();
    let (clips, rejected) = validate(raw, &sents, v.duration, l);
    if clips.is_empty() {
        bail!(
            "Claude found no clip that fits {:.0} to {:.0}s. Try wider limits.",
            l.min,
            l.max
        );
    }
    Ok((clips, rejected))
}

/// Moves one edge by `delta` seconds, or to the nearest sentence boundary when `to_sentence`.
pub fn nudge(
    c: &mut Clip,
    edge: &str,
    delta: f64,
    to_sentence: bool,
    sents: &[Range],
    duration: f64,
) {
    let pick = |t: f64, starts: bool| {
        sents
            .iter()
            .map(|s| if starts { s.start } else { s.end })
            .filter(|x| (x - t).abs() > 0.05)
            .min_by(|a, b| (a - t).abs().total_cmp(&(b - t).abs()))
            .unwrap_or(t)
    };
    match (edge, to_sentence) {
        ("start", true) => c.start = pick(c.start, true),
        ("end", true) => c.end = pick(c.end, false),
        ("start", false) => c.start = (c.start + delta).clamp(0.0, c.end - 1.0),
        (_, false) => c.end = (c.end + delta).clamp(c.start + 1.0, duration),
        _ => {}
    }
}

/// Video filter turning a landscape frame into 1080x1920 for each framing.
/// `bubble` is the camera bubble as fractions of the frame (x, y, w, h).
pub fn frame_filter(f: Framing, bubble: Option<(f64, f64, f64, f64)>) -> String {
    match (f, bubble) {
        (Framing::Center, _) => format!("scale=-2:{H},crop={W}:{H},setsar=1"),
        (Framing::Split, Some((x, y, w, h))) => {
            let top = H / 2;
            format!(
                "split[s][c];[s]scale=-2:{top},crop={W}:{top}[top];\
                 [c]crop=iw*{w:.4}:ih*{h:.4}:iw*{x:.4}:ih*{y:.4},scale={W}:{W}:force_original_aspect_ratio=increase,crop={W}:{top}[cam];\
                 [top][cam]vstack,setsar=1"
            )
        }
        // Split without a known bubble position falls back to blur fit.
        (Framing::BlurFit, _) | (Framing::Split, None) => format!(
            "split[bg][fg];[bg]scale={W}:{H}:force_original_aspect_ratio=increase,crop={W}:{H},gblur=sigma=40,colorchannelmixer=rr=0.8:gg=0.8:bb=0.8[b];\
             [fg]scale={W}:-2[f];[b][f]overlay=(W-w)/2:(H-h)/2,setsar=1"
        ),
    }
}

/// Groups clip words into short caption chunks (max 3 words, broken at sentence ends and pauses).
pub fn caption_chunks(words: &[Word]) -> Vec<Vec<Word>> {
    let mut out: Vec<Vec<Word>> = Vec::new();
    let mut cur: Vec<Word> = Vec::new();
    for w in words {
        let gap = cur.last().map(|l: &Word| w.start - l.end).unwrap_or(0.0);
        if !cur.is_empty() && (cur.len() >= 3 || gap > 0.6) {
            out.push(std::mem::take(&mut cur));
        }
        let ends = w.text.trim_end().ends_with(['.', '?', '!']);
        cur.push(w.clone());
        if ends {
            out.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn ass_time(t: f64) -> String {
    let cs = (t.max(0.0) * 100.0).round() as u64;
    format!(
        "{}:{:02}:{:02}.{:02}",
        cs / 360_000,
        cs / 6000 % 60,
        cs / 100 % 60,
        cs % 100
    )
}

fn ass_escape(s: &str) -> String {
    s.replace('\\', "")
        .replace('{', "(")
        .replace('}', ")")
        .to_uppercase()
}

/// An ASS script with one event per word: the chunk is shown with the spoken word highlighted.
/// `words` are already relative to the clip start.
pub fn ass_script(words: &[Word], highlight_bgr: &str) -> String {
    let mut s = format!(
        "[Script Info]\nScriptType: v4.00+\nPlayResX: {W}\nPlayResY: {H}\nWrapStyle: 0\n\n[V4+ Styles]\n\
         Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n\
         Style: Short,Helvetica Neue,92,&H00FFFFFF,&H00FFFFFF,&H00000000,&H64000000,-1,0,0,0,100,100,0,0,1,7,3,2,80,80,520,1\n\n\
         [Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n"
    );
    for chunk in caption_chunks(words) {
        for (i, w) in chunk.iter().enumerate() {
            let end = chunk
                .get(i + 1)
                .map(|n| n.start)
                .unwrap_or(w.end)
                .max(w.start + 0.05);
            let text: Vec<String> = chunk
                .iter()
                .enumerate()
                .map(|(j, x)| {
                    if j == i {
                        format!(
                            "{{\\c&H{highlight_bgr}&}}{}{{\\c&HFFFFFF&}}",
                            ass_escape(&x.text)
                        )
                    } else {
                        ass_escape(&x.text)
                    }
                })
                .collect();
            s += &format!(
                "Dialogue: 0,{},{},Short,,0,0,0,,{}\n",
                ass_time(w.start),
                ass_time(end),
                text.join(" ")
            );
        }
    }
    s
}

/// "#6d5efc" -> ASS colour "FC5E6D" (blue, green, red).
pub fn bgr(hex: &str) -> String {
    let h = hex.trim_start_matches('#');
    if h.len() != 6 {
        return "00FFFF".into();
    }
    format!("{}{}{}", &h[4..6], &h[2..4], &h[0..2]).to_uppercase()
}

fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.chars()
        .take(40)
        .collect::<String>()
        .trim_end_matches('-')
        .to_string()
}

#[derive(Debug, Serialize)]
pub struct Rendered {
    pub clip: String,
    pub file: PathBuf,
    pub captions: String,
}

/// Renders the chosen clips. Returns the files and a note about how captions were drawn.
pub async fn render(
    v: &Video,
    clip_ids: &[String],
    framing: Framing,
) -> Result<(Vec<Rendered>, Option<String>)> {
    let mut file = load(&v.id);
    let words = library::load_transcript(&v.id)
        .map(|t| t.words)
        .unwrap_or_default();
    let filters = media::run("ffmpeg", &["-hide_banner", "-filters"])
        .await
        .unwrap_or_default();
    let (has_ass, has_drawtext) = (filters.contains(" ass "), filters.contains(" drawtext "));
    let mut note = None;
    if !has_ass {
        note = Some(if has_drawtext {
            "Captions were drawn without word highlighting because this ffmpeg lacks libass."
                .to_string()
        } else {
            "This ffmpeg can't draw text, so each short has a .ass captions file next to it instead of burned-in captions.".to_string()
        });
    }
    let src = library::playable_path(&v.id);
    std::fs::create_dir_all(dir(&v.id))?;
    let mut out = Vec::new();
    for id in clip_ids {
        let n = file
            .clips
            .iter()
            .position(|c| &c.id == id)
            .with_context(|| format!("no clip {id}"))?;
        let c = file.clips[n].clone();
        let base = dir(&v.id).join(format!("{:02}-{}", n + 1, slug(&c.title)));
        let rel: Vec<Word> = words
            .iter()
            .filter(|w| w.start >= c.start - 0.05 && w.end <= c.end + 0.05)
            .map(|w| Word {
                start: w.start - c.start,
                end: w.end - c.start,
                text: w.text.clone(),
            })
            .collect();
        let ass = base.with_extension("ass");
        std::fs::write(&ass, ass_script(&rel, &bgr(HIGHLIGHT)))?;
        let mut vf = frame_filter(framing, v.bubble);
        if has_ass {
            vf += &format!(
                ",ass='{}'",
                ass.to_string_lossy()
                    .replace('\'', "\\'")
                    .replace(':', "\\:")
            );
        } else if has_drawtext {
            for chunk in caption_chunks(&rel) {
                let text = chunk
                    .iter()
                    .map(|w| ass_escape(&w.text))
                    .collect::<Vec<_>>()
                    .join(" ")
                    .replace('\'', "")
                    .replace(':', "\\:");
                let (a, b) = (chunk[0].start, chunk[chunk.len() - 1].end);
                vf += &format!(
                    ",drawtext=text='{text}':fontsize=84:fontcolor=white:borderw=6:bordercolor=black:x=(w-text_w)/2:y=h*0.7:enable='between(t,{a:.2},{b:.2})'"
                );
            }
        }
        let window = Some((c.start, c.end - c.start));
        let af = match voice::chain(&v.voice, None) {
            Some(_) if v.voice.auto => {
                let a = voice::analyze(&src, None, window).await.ok();
                voice::chain(&v.voice, a.as_ref()).unwrap_or_else(|| "anull".into())
            }
            Some(chain) => chain,
            None => "anull".into(),
        };
        let mp4 = base.with_extension("mp4");
        let (ss, t) = (format!("{:.3}", c.start), format!("{:.3}", c.end - c.start));
        media::run(
            "ffmpeg",
            &[
                "-y",
                "-ss",
                &ss,
                "-t",
                &t,
                "-i",
                &src.to_string_lossy(),
                "-filter_complex",
                &format!("[0:v]{vf}[v];[0:a]{af}[a]"),
                "-map",
                "[v]",
                "-map",
                "[a]",
                "-c:v",
                "h264_videotoolbox",
                "-b:v",
                "8M",
                "-r",
                "30",
                "-c:a",
                "aac",
                "-b:a",
                "160k",
                "-movflags",
                "+faststart",
                &mp4.to_string_lossy(),
            ],
        )
        .await?;
        let post = format!("{}\n\n{}\n", c.title, c.caption);
        std::fs::write(base.with_extension("txt"), &post)?;
        if has_ass {
            let _ = std::fs::remove_file(&ass);
        }
        out.push(Rendered {
            clip: c.id.clone(),
            file: mp4,
            captions: post,
        });
    }
    file.rendered = out
        .iter()
        .map(|r| r.file.to_string_lossy().to_string())
        .collect();
    save(&v.id, &file)?;
    Ok((out, note))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clip(id: &str, start: f64, end: f64, score: u8) -> Clip {
        Clip {
            id: id.into(),
            start,
            end,
            title: "Hook".into(),
            reason: "r".into(),
            score,
            score_reason: "s".into(),
            caption: "c".into(),
            selected: true,
        }
    }
    fn sents() -> Vec<Range> {
        (0..20)
            .map(|i| Range {
                start: i as f64 * 10.0,
                end: i as f64 * 10.0 + 9.5,
            })
            .collect()
    }

    #[test]
    fn validator_enforces_bounds_length_and_overlap() {
        let l = Limits {
            count: 3,
            min: 15.0,
            max: 40.0,
            allow_overlap: false,
        };
        let raw = vec![
            clip("a", 10.0, 39.5, 9),   // ok, 29.5s
            clip("b", 30.0, 59.5, 8),   // overlaps a
            clip("c", 100.0, 109.5, 7), // too short
            clip("d", 150.0, 260.0, 9), // past the end of a 200s video once snapped? too long
            clip("e", 60.0, 89.5, 5),   // ok
        ];
        let (kept, rejected) = validate(raw, &sents(), 200.0, &l);
        assert_eq!(
            kept.iter().map(|c| (c.start, c.end)).collect::<Vec<_>>(),
            vec![(10.0, 39.5), (60.0, 89.5)]
        );
        assert_eq!(kept[0].id, "c1");
        assert_eq!(rejected.len(), 3);
        let l2 = Limits {
            allow_overlap: true,
            ..l
        };
        let (kept, _) = validate(
            vec![clip("a", 10.0, 39.5, 9), clip("b", 30.0, 59.5, 8)],
            &sents(),
            200.0,
            &l2,
        );
        assert_eq!(kept.len(), 2);
    }

    #[test]
    fn frame_filters_for_each_mode() {
        assert_eq!(
            frame_filter(Framing::Center, None),
            "scale=-2:1920,crop=1080:1920,setsar=1"
        );
        let blur = frame_filter(Framing::BlurFit, None);
        assert!(blur.contains("gblur=sigma=40") && blur.contains("overlay=(W-w)/2:(H-h)/2"));
        let split = frame_filter(Framing::Split, Some((0.02, 0.7, 0.15, 0.25)));
        assert!(
            split.contains("crop=iw*0.1500:ih*0.2500:iw*0.0200:ih*0.7000")
                && split.contains("vstack")
        );
        assert_eq!(frame_filter(Framing::Split, None), blur);
    }

    #[test]
    fn caption_chunks_break_on_count_pauses_and_sentences() {
        let w = |s: f64, t: &str| Word {
            start: s,
            end: s + 0.3,
            text: t.into(),
        };
        let words = vec![
            w(0.0, "one"),
            w(0.3, "two"),
            w(0.6, "three"),
            w(0.9, "four."),
            w(1.2, "five"),
            w(3.0, "six"),
        ];
        let c: Vec<Vec<String>> = caption_chunks(&words)
            .iter()
            .map(|c| c.iter().map(|w| w.text.clone()).collect())
            .collect();
        assert_eq!(
            c,
            vec![
                vec!["one", "two", "three"],
                vec!["four."],
                vec!["five"],
                vec!["six"]
            ]
        );
    }

    #[test]
    fn ass_highlights_one_word_per_event() {
        let words = vec![
            Word {
                start: 0.0,
                end: 0.4,
                text: "big".into(),
            },
            Word {
                start: 0.4,
                end: 0.9,
                text: "idea".into(),
            },
        ];
        let s = ass_script(&words, "00FFFF");
        assert!(s.contains(
            "Dialogue: 0,0:00:00.00,0:00:00.40,Short,,0,0,0,,{\\c&H00FFFF&}BIG{\\c&HFFFFFF&} IDEA"
        ));
        assert!(s.contains(
            "Dialogue: 0,0:00:00.40,0:00:00.90,Short,,0,0,0,,BIG {\\c&H00FFFF&}IDEA{\\c&HFFFFFF&}"
        ));
        assert_eq!(bgr("#6d5efc"), "FC5E6D");
    }

    #[test]
    fn nudges_by_time_and_to_sentences() {
        let mut c = clip("a", 10.0, 39.5, 5);
        nudge(&mut c, "start", 0.5, false, &sents(), 200.0);
        assert_eq!(c.start, 10.5);
        nudge(&mut c, "end", 0.0, true, &sents(), 200.0);
        assert!(c.end == 29.5 || c.end == 49.5);
        assert_eq!(slug("Why 90% of GTM fails!"), "why-90-of-gtm-fails");
    }
}
