//! Edit plans: a small, fixed vocabulary of edits that Claude (or an agent) proposes and the user
//! previews as a readable diff before anything renders. Applying keeps a one-step undo snapshot.
use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::ai::{self, mmss};
use crate::cuts::{Range, Segment, Word, map_time};
use crate::library::{self, Chapter, Note, Transcript, Video};
use crate::voice::Voice;
use crate::{media, voice};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Op {
    Cut {
        start: f64,
        end: f64,
        #[serde(default)]
        label: String,
    },
    KeepOnly {
        ranges: Vec<Range>,
        #[serde(default)]
        label: String,
    },
    Speed {
        start: f64,
        end: f64,
        factor: f64,
    },
    SetTitle {
        title: String,
    },
    SetChapters {
        chapters: Vec<Chapter>,
    },
    Voice {
        #[serde(default)]
        gain_db: Option<f64>,
        #[serde(default)]
        auto: Option<bool>,
    },
    AddNote {
        time: f64,
        text: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub ops: Vec<Op>,
    #[serde(default)]
    pub explanation: String,
}

const MAX_OPS: usize = 30;
const EPS: f64 = 0.05;

fn check_range(start: f64, end: f64, duration: f64, what: &str) -> Result<()> {
    ensure!(
        start.is_finite() && end.is_finite(),
        "{what}: times must be numbers"
    );
    ensure!(
        start >= 0.0 && end <= duration + EPS,
        "{what}: {} to {} is outside the video (0:00 to {})",
        mmss(start),
        mmss(end),
        mmss(duration)
    );
    ensure!(
        end - start >= 0.1,
        "{what}: the range {} to {} is empty or backwards",
        mmss(start),
        mmss(end)
    );
    Ok(())
}

fn overlaps(a: &[(f64, f64)]) -> Option<(f64, f64)> {
    let mut v = a.to_vec();
    v.sort_by(|x, y| x.0.total_cmp(&y.0));
    v.windows(2)
        .find(|w| w[1].0 < w[0].1 - 1e-9)
        .map(|w| (w[1].0, w[0].1))
}

/// Parses and checks a plan against the video. Anything outside the vocabulary is rejected.
pub fn validate(raw: &Value, duration: f64) -> Result<Plan> {
    let plan: Plan = serde_json::from_value(raw.clone())
        .map_err(|e| anyhow::anyhow!("invalid edit plan: {e}"))?;
    ensure!(!plan.ops.is_empty(), "the plan has no edits");
    ensure!(
        plan.ops.len() <= MAX_OPS,
        "too many edits in one plan (max {MAX_OPS})"
    );
    let mut cuts = Vec::new();
    let mut speeds = Vec::new();
    let mut keep_only = 0;
    for op in &plan.ops {
        match op {
            Op::Cut { start, end, .. } => {
                check_range(*start, *end, duration, "cut")?;
                cuts.push((*start, *end));
            }
            Op::KeepOnly { ranges, .. } => {
                keep_only += 1;
                ensure!(!ranges.is_empty(), "keep_only needs at least one range");
                for r in ranges {
                    check_range(r.start, r.end, duration, "keep_only")?;
                }
                let rs: Vec<(f64, f64)> = ranges.iter().map(|r| (r.start, r.end)).collect();
                if let Some((a, b)) = overlaps(&rs) {
                    bail!("keep_only ranges overlap around {} to {}", mmss(a), mmss(b));
                }
            }
            Op::Speed { start, end, factor } => {
                check_range(*start, *end, duration, "speed")?;
                ensure!(
                    (0.25..=4.0).contains(factor),
                    "speed factor {factor} must be between 0.25 and 4"
                );
                speeds.push((*start, *end));
            }
            Op::SetTitle { title } => ensure!(
                !title.trim().is_empty() && title.chars().count() <= 120,
                "title must be 1 to 120 characters"
            ),
            Op::SetChapters { chapters } => {
                for c in chapters {
                    ensure!(
                        c.start >= 0.0 && c.start < duration && !c.title.trim().is_empty(),
                        "chapter \"{}\" is outside the video or untitled",
                        c.title
                    );
                }
            }
            Op::Voice { gain_db, auto } => {
                ensure!(
                    gain_db.is_some() || auto.is_some(),
                    "voice needs gain_db or auto"
                );
                if let Some(g) = gain_db {
                    ensure!(
                        (-12.0..=12.0).contains(g),
                        "voice gain {g} dB must be between -12 and +12"
                    );
                }
            }
            Op::AddNote { time, text } => ensure!(
                *time >= 0.0 && *time <= duration && !text.trim().is_empty(),
                "note at {} is outside the video or empty",
                mmss(*time)
            ),
        }
    }
    ensure!(keep_only <= 1, "use a single keep_only");
    if let Some((a, b)) = overlaps(&cuts) {
        bail!("cuts overlap around {} to {}", mmss(a), mmss(b));
    }
    if let Some((a, b)) = overlaps(&speeds) {
        bail!("speed changes overlap around {} to {}", mmss(a), mmss(b));
    }
    Ok(plan)
}

/// The pieces of the source that survive the plan, with their playback speed.
pub fn timeline(plan: &Plan, duration: f64) -> Vec<Segment> {
    let mut keep: Vec<(f64, f64)> = vec![(0.0, duration)];
    for op in &plan.ops {
        if let Op::KeepOnly { ranges, .. } = op {
            let mut r: Vec<(f64, f64)> = ranges
                .iter()
                .map(|r| (r.start, r.end.min(duration)))
                .collect();
            r.sort_by(|a, b| a.0.total_cmp(&b.0));
            keep = r;
        }
    }
    for op in &plan.ops {
        if let Op::Cut { start, end, .. } = op {
            keep = keep
                .into_iter()
                .flat_map(|(a, b)| {
                    let mut out = Vec::new();
                    if *start > a {
                        out.push((a, start.min(b)));
                    }
                    if *end < b {
                        out.push((end.max(a), b));
                    }
                    out
                })
                .filter(|(a, b)| b - a > 0.05)
                .collect();
        }
    }
    // Split kept pieces at speed boundaries.
    let speeds: Vec<(f64, f64, f64)> = plan
        .ops
        .iter()
        .filter_map(|o| {
            if let Op::Speed { start, end, factor } = o {
                Some((*start, *end, *factor))
            } else {
                None
            }
        })
        .collect();
    let mut segs = Vec::new();
    for (a, b) in keep {
        let mut cuts: Vec<f64> = vec![a, b];
        for (s, e, _) in &speeds {
            for t in [*s, *e] {
                if t > a && t < b {
                    cuts.push(t);
                }
            }
        }
        cuts.sort_by(|x, y| x.total_cmp(y));
        cuts.dedup();
        for w in cuts.windows(2) {
            let mid = (w[0] + w[1]) / 2.0;
            let speed = speeds
                .iter()
                .find(|(s, e, _)| mid >= *s && mid <= *e)
                .map(|x| x.2)
                .unwrap_or(1.0);
            if w[1] - w[0] > 0.05 {
                segs.push(Segment {
                    start: w[0],
                    end: w[1],
                    speed,
                });
            }
        }
    }
    segs
}

fn changes_picture(plan: &Plan) -> bool {
    plan.ops
        .iter()
        .any(|o| matches!(o, Op::Cut { .. } | Op::KeepOnly { .. } | Op::Speed { .. }))
}

/// Human-readable lines, e.g. "Cutting 0:42 to 1:15, \"pricing tiers\"".
pub fn describe(plan: &Plan, duration: f64) -> Vec<String> {
    let mut out: Vec<String> = plan
        .ops
        .iter()
        .map(|op| match op {
            Op::Cut { start, end, label } => format!(
                "Cutting {} to {}{}",
                mmss(*start),
                mmss(*end),
                quoted(label)
            ),
            Op::KeepOnly { ranges, label } => format!(
                "Keeping only {}{}",
                ranges
                    .iter()
                    .map(|r| format!("{} to {}", mmss(r.start), mmss(r.end)))
                    .collect::<Vec<_>>()
                    .join(", "),
                quoted(label)
            ),
            Op::Speed { start, end, factor } => {
                format!("Playing {} to {} at {factor}x", mmss(*start), mmss(*end))
            }
            Op::SetTitle { title } => format!("Renaming to \"{title}\""),
            Op::SetChapters { chapters } => format!(
                "Chapters: {}",
                chapters
                    .iter()
                    .map(|c| format!("{} {}", mmss(c.start), c.title))
                    .collect::<Vec<_>>()
                    .join(" · ")
            ),
            Op::Voice { gain_db, auto } => match (gain_db, auto) {
                (Some(g), Some(true)) => format!("Voice: Auto level, then {g:+.0} dB"),
                (_, Some(true)) => "Voice: Auto level".into(),
                (Some(g), _) => format!("Voice: {g:+.0} dB"),
                _ => "Voice: off".into(),
            },
            Op::AddNote { time, text } => format!("Note at {}: {text}", mmss(*time)),
        })
        .collect();
    if changes_picture(plan) {
        let new_len: f64 = timeline(plan, duration).iter().map(|s| s.out_len()).sum();
        out.push(format!("Length {} → {}", mmss(duration), mmss(new_len)));
    }
    out
}

fn quoted(label: &str) -> String {
    if label.trim().is_empty() {
        String::new()
    } else {
        format!(", \"{}\"", label.trim())
    }
}

/// Moves words through the new timeline: cut words go, sped-up words get shorter.
pub fn remap_words(words: &[Word], segs: &[Segment]) -> Vec<Word> {
    words
        .iter()
        .filter_map(|w| {
            let mid = (w.start + w.end) / 2.0;
            let seg = segs.iter().find(|s| mid >= s.start && mid <= s.end)?;
            let start = map_time(segs, w.start.max(seg.start))?;
            let end = map_time(segs, w.end.min(seg.end))?;
            Some(Word {
                start,
                end,
                text: w.text.clone(),
            })
        })
        .collect()
}

fn undo_dir(id: &str) -> std::path::PathBuf {
    library::dir(id).join("undo")
}

pub fn can_undo(id: &str) -> bool {
    library::exists(&undo_dir(id).join("meta.json"))
}

fn snapshot(id: &str) -> Result<()> {
    let d = undo_dir(id);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d)?;
    for f in ["video.mp4", "meta.json", "transcript.json"] {
        let src = library::dir(id).join(f);
        if src.exists() {
            std::fs::copy(&src, d.join(f))?;
        }
    }
    Ok(())
}

pub async fn undo(id: &str) -> Result<Video> {
    let d = undo_dir(id);
    ensure!(can_undo(id), "nothing to undo");
    for f in ["video.mp4", "meta.json", "transcript.json"] {
        let dst = library::dir(id).join(f);
        if d.join(f).exists() {
            std::fs::copy(d.join(f), &dst)?;
        } else {
            let _ = std::fs::remove_file(&dst);
        }
    }
    std::fs::remove_dir_all(&d)?;
    let v = library::load(id)?;
    voice::render(id, &v.voice).await?;
    let _ = media::thumbnail(
        &library::video_path(id),
        &library::thumb_path(id),
        (v.duration * 0.1).min(2.0),
    )
    .await;
    Ok(v)
}

/// Renders the plan. The previous state is saved first so Undo restores it exactly.
pub async fn apply(id: &str, plan: &Plan) -> Result<Video> {
    let mut v = library::load(id)?;
    let plan = validate(&serde_json::to_value(plan)?, v.duration)?;
    snapshot(id)?;
    let mut segs = vec![Segment {
        start: 0.0,
        end: v.duration,
        speed: 1.0,
    }];
    if changes_picture(&plan) {
        segs = timeline(&plan, v.duration);
        ensure!(!segs.is_empty(), "the plan would remove the whole video");
        let src = library::video_path(id);
        let tmp = library::dir(id).join("edit.mp4");
        media::render_timeline(&src, &tmp, &segs).await?;
        tokio::fs::rename(&tmp, &src).await?;
        v.duration = media::duration(&src).await?;
        v.edited = true;
        if let Some(t) = library::load_transcript(id) {
            let words = remap_words(&t.words, &segs);
            let text = words
                .iter()
                .map(|w| w.text.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            library::save_transcript(id, &Transcript { words, text })?;
        }
        v.chapters = v
            .chapters
            .iter()
            .filter_map(|c| {
                map_time(&segs, c.start).map(|start| Chapter {
                    start,
                    title: c.title.clone(),
                })
            })
            .collect();
        v.notes = v
            .notes
            .iter()
            .filter_map(|n| {
                map_time(&segs, n.time).map(|time| Note {
                    time,
                    text: n.text.clone(),
                })
            })
            .collect();
    }
    for op in &plan.ops {
        match op {
            Op::SetTitle { title } => v.title = title.trim().to_string(),
            // Chapter and note times are in the original video's timeline.
            Op::SetChapters { chapters } => {
                v.chapters = chapters
                    .iter()
                    .filter_map(|c| {
                        map_time(&segs, c.start).map(|start| Chapter {
                            start,
                            title: c.title.trim().to_string(),
                        })
                    })
                    .collect();
                v.chapters.sort_by(|a, b| a.start.total_cmp(&b.start));
            }
            Op::AddNote { time, text } => {
                if let Some(t) = map_time(&segs, *time) {
                    v.notes.push(Note {
                        time: t,
                        text: text.trim().to_string(),
                    });
                }
            }
            Op::Voice { gain_db, auto } => {
                v.voice = Voice {
                    gain_db: gain_db.unwrap_or(v.voice.gain_db),
                    auto: auto.unwrap_or(v.voice.auto),
                }
                .clamped();
            }
            _ => {}
        }
    }
    library::save(&v)?;
    voice::render(id, &v.voice).await?;
    let _ = media::thumbnail(
        &library::video_path(id),
        &library::thumb_path(id),
        (v.duration * 0.1).min(2.0),
    )
    .await;
    Ok(v)
}

pub fn plan_schema() -> Value {
    let range = json!({"type": "object", "additionalProperties": false, "required": ["start", "end"],
        "properties": {"start": {"type": "number"}, "end": {"type": "number"}}});
    let op = |name: &str, props: Value, required: Vec<&str>| {
        let mut p = props.as_object().cloned().unwrap_or_default();
        p.insert("op".into(), json!({"const": name}));
        let mut req = vec!["op"];
        req.extend(required);
        json!({"type": "object", "additionalProperties": false, "required": req, "properties": p})
    };
    json!({
        "type": "object", "additionalProperties": false, "required": ["ops", "explanation"],
        "properties": {
            "explanation": {"type": "string"},
            "ops": {"type": "array", "items": {"anyOf": [
                op("cut", json!({"start": {"type": "number"}, "end": {"type": "number"}, "label": {"type": "string"}}), vec!["start", "end"]),
                op("keep_only", json!({"ranges": {"type": "array", "items": range}, "label": {"type": "string"}}), vec!["ranges"]),
                op("speed", json!({"start": {"type": "number"}, "end": {"type": "number"}, "factor": {"type": "number"}}), vec!["start", "end", "factor"]),
                op("set_title", json!({"title": {"type": "string"}}), vec!["title"]),
                op("set_chapters", json!({"chapters": {"type": "array", "items": {"type": "object", "additionalProperties": false, "required": ["start", "title"],
                    "properties": {"start": {"type": "number"}, "title": {"type": "string"}}}}}), vec!["chapters"]),
                op("voice", json!({"gain_db": {"type": "number"}, "auto": {"type": "boolean"}}), vec![]),
                op("add_note", json!({"time": {"type": "number"}, "text": {"type": "string"}}), vec!["time", "text"]),
            ]}}
        }
    })
}

/// Lines like "[12.40-15.85] text" so Claude can pick exact times.
pub fn segment_transcript(t: &Transcript) -> String {
    library::cues(&t.words)
        .into_iter()
        .map(|(a, b, text)| format!("[{a:.2}-{b:.2}] {text}"))
        .collect::<Vec<_>>()
        .join("\n")
}

pub async fn plan_from_prompt(v: &Video, t: Option<&Transcript>, prompt: &str) -> Result<Plan> {
    let system = "You turn a person's editing request into an edit plan for one screen recording. \
        Use only the ops in the schema. Times are seconds in the CURRENT video. \
        Cut on the transcript line boundaries given. Only include what was asked for; if a request \
        cannot be done with these ops, return an empty ops list and say why in explanation.";
    let state = json!({"duration": v.duration, "title": v.title, "chapters": v.chapters, "voice": v.voice, "notes": v.notes});
    let transcript = t
        .map(segment_transcript)
        .unwrap_or_else(|| "(no transcript yet)".into());
    let request: String = prompt.chars().take(600).collect();
    let user = format!(
        "Current state: {state}\n\nTranscript lines [start-end] in seconds:\n{transcript}\n\nRequest: {request}"
    );
    let raw = ai::ask_json(system, &user, &plan_schema()).await?;
    if raw["ops"].as_array().is_some_and(|a| a.is_empty()) {
        bail!(
            "{}",
            raw["explanation"]
                .as_str()
                .unwrap_or("Claude could not turn that into an edit.")
        );
    }
    validate(&raw, v.duration)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(v: Value) -> Result<Plan> {
        validate(&v, 60.0)
    }

    #[test]
    fn accepts_a_good_plan() {
        let p = plan(json!({"explanation": "x", "ops": [
            {"op": "cut", "start": 0, "end": 5, "label": "intro"},
            {"op": "speed", "start": 20, "end": 30, "factor": 2},
            {"op": "set_title", "title": "Billing tour"},
            {"op": "voice", "auto": true},
            {"op": "add_note", "time": 12, "text": "Check this"}
        ]}))
        .unwrap();
        assert_eq!(p.ops.len(), 5);
        let d = describe(&p, 60.0);
        assert_eq!(d[0], "Cutting 0:00 to 0:05, \"intro\"");
        assert_eq!(d.last().unwrap(), "Length 1:00 → 0:50");
    }

    #[test]
    fn rejects_out_of_range_and_backwards() {
        assert!(
            plan(json!({"ops": [{"op": "cut", "start": 50, "end": 70}]}))
                .unwrap_err()
                .to_string()
                .contains("outside the video")
        );
        assert!(
            plan(json!({"ops": [{"op": "cut", "start": 10, "end": 5}]}))
                .unwrap_err()
                .to_string()
                .contains("backwards")
        );
        assert!(
            plan(json!({"ops": [{"op": "speed", "start": 0, "end": 5, "factor": 9}]})).is_err()
        );
        assert!(plan(json!({"ops": [{"op": "voice", "gain_db": 30}]})).is_err());
    }

    #[test]
    fn rejects_overlapping_cuts() {
        let e = plan(json!({"ops": [{"op": "cut", "start": 0, "end": 10}, {"op": "cut", "start": 8, "end": 12}]})).unwrap_err();
        assert!(e.to_string().contains("overlap"));
    }

    #[test]
    fn rejects_unknown_ops_and_fields() {
        assert!(plan(json!({"ops": [{"op": "delete_everything"}]})).is_err());
        assert!(
            plan(json!({"ops": [{"op": "cut", "start": 0, "end": 1, "shell": "rm -rf"}]})).is_err()
        );
        assert!(plan(json!({"ops": []})).is_err());
    }

    #[test]
    fn keep_only_then_cut_then_speed_timeline() {
        let p = plan(json!({"ops": [
            {"op": "keep_only", "ranges": [{"start": 10, "end": 40}]},
            {"op": "cut", "start": 20, "end": 25},
            {"op": "speed", "start": 30, "end": 40, "factor": 2}
        ]}))
        .unwrap();
        let t = timeline(&p, 60.0);
        assert_eq!(
            t,
            vec![
                Segment {
                    start: 10.0,
                    end: 20.0,
                    speed: 1.0
                },
                Segment {
                    start: 25.0,
                    end: 30.0,
                    speed: 1.0
                },
                Segment {
                    start: 30.0,
                    end: 40.0,
                    speed: 2.0
                },
            ]
        );
        assert_eq!(map_time(&t, 35.0), Some(17.5));
        assert_eq!(map_time(&t, 22.0), None);
    }

    #[test]
    fn remaps_words_through_cuts_and_speed() {
        let segs = vec![
            Segment {
                start: 0.0,
                end: 2.0,
                speed: 1.0,
            },
            Segment {
                start: 4.0,
                end: 8.0,
                speed: 2.0,
            },
        ];
        let words = vec![
            Word {
                start: 0.5,
                end: 1.0,
                text: "kept".into(),
            },
            Word {
                start: 2.5,
                end: 3.0,
                text: "cut".into(),
            },
            Word {
                start: 5.0,
                end: 6.0,
                text: "fast".into(),
            },
        ];
        let w = remap_words(&words, &segs);
        assert_eq!(w.len(), 2);
        assert_eq!((w[1].start, w[1].end), (2.5, 3.0));
    }
}
