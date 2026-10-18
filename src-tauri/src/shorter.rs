//! "Make it shorter": Claude picks the sentences that carry the core message, cut to a target
//! length. The result renders as a new library video next to the original.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::ai;
use crate::capture;
use crate::cuts::{self, Range, Segment};
use crate::edits;
use crate::library::{self, Transcript, Video};
use crate::media;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShortPlan {
    pub ranges: Vec<Range>,
    pub length: f64,
    pub explanation: String,
}

/// Snaps to sentences, merges touching ranges and checks the length lands near the target.
pub fn finish_plan(
    ranges: Vec<Range>,
    sents: &[Range],
    duration: f64,
    target: f64,
) -> Result<Vec<Range>> {
    let mut r: Vec<Range> = ranges
        .into_iter()
        .filter(|r| r.start.is_finite() && r.end > r.start)
        .map(|r| {
            cuts::snap_to_sentences(
                Range {
                    start: r.start.max(0.0),
                    end: r.end.min(duration),
                },
                sents,
            )
        })
        .collect();
    r.sort_by(|a, b| a.start.total_cmp(&b.start));
    let mut merged: Vec<Range> = Vec::new();
    for x in r {
        match merged.last_mut() {
            Some(l) if x.start <= l.end + 0.05 => l.end = l.end.max(x.end),
            _ => merged.push(x),
        }
    }
    let len: f64 = merged.iter().map(|r| r.end - r.start).sum();
    ensure!(!merged.is_empty(), "Claude picked nothing to keep");
    ensure!(
        len <= target * 1.15 + 1.0,
        "the picks run {:.0}s, well over the {:.0}s target",
        len,
        target
    );
    Ok(merged)
}

/// One line per sentence, so Claude can only cut between sentences.
pub fn sentence_transcript(t: &Transcript) -> String {
    cuts::sentences(&t.words)
        .iter()
        .map(|r| {
            let text: Vec<&str> = t
                .words
                .iter()
                .filter(|w| w.start >= r.start - 1e-6 && w.end <= r.end + 1e-6)
                .map(|w| w.text.as_str())
                .collect();
            format!("[{:.2}-{:.2}] {}", r.start, r.end, text.join(" "))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub async fn plan(v: &Video, t: &Transcript, target: f64) -> Result<ShortPlan> {
    ensure!(
        target >= 5.0 && target < v.duration,
        "pick a target shorter than the video"
    );
    let schema = json!({"type": "object", "additionalProperties": false, "required": ["ranges", "explanation"],
        "properties": {"explanation": {"type": "string"}, "ranges": {"type": "array", "items": {"type": "object",
            "additionalProperties": false, "required": ["start", "end"],
            "properties": {"start": {"type": "number"}, "end": {"type": "number"}}}}}});
    let system = "You shorten a screen recording to its core message. Keep whole sentences, in order. \
        Prefer the parts that state the point, the key steps and the conclusion; drop warm-up, repeats and tangents.";
    let user = format!(
        "Video length {:.1}s. Target about {:.0}s (do not exceed it by more than a few seconds).\n\
         Transcript lines [start-end] in seconds:\n{}\n\nReturn the ranges to KEEP, using the line boundaries.",
        v.duration,
        target,
        edits::segment_transcript(t)
    );
    let sents = cuts::sentences(&t.words);
    let mut raw = ai::ask_json(system, &user, &schema).await?;
    let mut ranges = finish_plan(
        serde_json::from_value(raw["ranges"].clone())?,
        &sents,
        v.duration,
        target,
    );
    // One retry with the overrun spelled out; Claude tends to keep "just one more" point.
    if let Err(e) = &ranges {
        let retry = format!(
            "{user}\n\nYour previous pick was rejected: {e}. Stay at or under {target:.0}s, dropping whole sentences."
        );
        raw = ai::ask_json(system, &retry, &schema).await?;
        ranges = finish_plan(
            serde_json::from_value(raw["ranges"].clone())?,
            &sents,
            v.duration,
            target,
        );
    }
    let ranges = ranges?;
    Ok(ShortPlan {
        length: ranges.iter().map(|r| r.end - r.start).sum(),
        ranges,
        explanation: raw["explanation"].as_str().unwrap_or_default().to_string(),
    })
}

/// Renders the kept ranges into a new library video and carries over the remapped transcript.
pub async fn render(v: &Video, ranges: &[Range]) -> Result<Video> {
    let segs: Vec<Segment> = ranges
        .iter()
        .map(|r| Segment {
            start: r.start,
            end: r.end,
            speed: 1.0,
        })
        .collect();
    let id = format!("{}-short", library::new_id());
    std::fs::create_dir_all(library::dir(&id))?;
    let part = library::dir(&id).join("segment-0.mp4");
    media::render_timeline(&library::video_path(&v.id), &part, &segs).await?;
    let mut short = capture::finish(&id, &[part]).await?;
    short.title = format!("{} (short)", v.title);
    short.voice = v.voice;
    if let Some(t) = library::load_transcript(&v.id) {
        let words = edits::remap_words(&t.words, &segs);
        let text = words
            .iter()
            .map(|w| w.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        short.has_transcript = !words.is_empty();
        library::save_transcript(&id, &Transcript { words, text })?;
    }
    library::save(&short)?;
    crate::voice::render(&id, &short.voice).await?;
    Ok(short)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snaps_merges_and_enforces_target() {
        let sents = vec![
            Range {
                start: 0.0,
                end: 4.0,
            },
            Range {
                start: 4.2,
                end: 9.0,
            },
            Range {
                start: 12.0,
                end: 20.0,
            },
        ];
        let r = finish_plan(
            vec![
                Range {
                    start: 0.2,
                    end: 3.9,
                },
                Range {
                    start: 4.1,
                    end: 8.8,
                },
            ],
            &sents,
            20.0,
            10.0,
        )
        .unwrap();
        assert_eq!(
            r,
            vec![
                Range {
                    start: 0.0,
                    end: 4.0
                },
                Range {
                    start: 4.2,
                    end: 9.0
                }
            ]
        );
        assert!(
            finish_plan(
                vec![Range {
                    start: 0.0,
                    end: 20.0
                }],
                &sents,
                20.0,
                5.0
            )
            .is_err()
        );
    }
}
