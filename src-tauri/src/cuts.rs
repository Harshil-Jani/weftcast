//! Builds the list of time ranges to KEEP when removing filler words and long silences.
//! Pure logic, so it is unit tested without ffmpeg.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Range {
    pub start: f64,
    pub end: f64,
}

/// A kept piece of the source, played back at `speed` (1 = normal).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Segment {
    pub start: f64,
    pub end: f64,
    pub speed: f64,
}

impl Segment {
    pub fn out_len(&self) -> f64 {
        (self.end - self.start) / self.speed
    }
}

/// Maps a source time to its time in the edited video, or None if it was cut.
pub fn map_time(segs: &[Segment], t: f64) -> Option<f64> {
    let mut out = 0.0;
    for s in segs {
        if t >= s.start && t <= s.end {
            return Some(out + (t - s.start) / s.speed);
        }
        out += s.out_len();
    }
    None
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Word {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CutOptions {
    pub remove_fillers: bool,
    pub remove_silences: bool,
    /// Silences shorter than this are left alone (seconds).
    pub min_silence: f64,
    /// Breathing room kept on each side of a removed silence (seconds).
    pub pad: f64,
}

impl Default for CutOptions {
    fn default() -> Self {
        Self {
            remove_fillers: true,
            remove_silences: true,
            min_silence: 0.8,
            pad: 0.2,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CutPlan {
    pub keep: Vec<Range>,
    pub removed: Vec<(Range, String)>,
    pub original: f64,
    pub result: f64,
}

const FILLERS: &[&str] = &[
    "um", "umm", "uh", "uhh", "uhm", "erm", "er", "ah", "hmm", "mm", "mhm",
];

pub fn is_filler(word: &str) -> bool {
    let w: String = word
        .chars()
        .filter(|c| c.is_alphabetic())
        .collect::<String>()
        .to_lowercase();
    !w.is_empty() && FILLERS.contains(&w.as_str())
}

/// `silences` come from ffmpeg silencedetect; `words` from the transcript.
pub fn plan(duration: f64, words: &[Word], silences: &[Range], opts: &CutOptions) -> CutPlan {
    let mut removed: Vec<(Range, String)> = Vec::new();
    if opts.remove_fillers {
        for w in words.iter().filter(|w| is_filler(&w.text)) {
            removed.push((
                Range {
                    start: w.start,
                    end: w.end,
                },
                format!("filler \"{}\"", w.text.trim()),
            ));
        }
    }
    if opts.remove_silences {
        for s in silences
            .iter()
            .filter(|s| s.end - s.start >= opts.min_silence)
        {
            let r = Range {
                start: s.start + opts.pad,
                end: s.end - opts.pad,
            };
            if r.end > r.start {
                removed.push((r, format!("silence {:.1}s", s.end - s.start)));
            }
        }
    }
    removed.retain(|(r, _)| r.end > r.start && r.start < duration);
    removed.sort_by(|a, b| a.0.start.total_cmp(&b.0.start));

    // Merge overlapping removals, then invert into keep ranges.
    let mut merged: Vec<Range> = Vec::new();
    for (r, _) in &removed {
        let r = Range {
            start: r.start.max(0.0),
            end: r.end.min(duration),
        };
        match merged.last_mut() {
            Some(last) if r.start <= last.end => last.end = last.end.max(r.end),
            _ => merged.push(r),
        }
    }
    let mut keep = Vec::new();
    let mut cursor = 0.0;
    for r in &merged {
        if r.start - cursor > 0.05 {
            keep.push(Range {
                start: cursor,
                end: r.start,
            });
        }
        cursor = r.end;
    }
    if duration - cursor > 0.05 {
        keep.push(Range {
            start: cursor,
            end: duration,
        });
    }
    let result = keep.iter().map(|r| r.end - r.start).sum();
    CutPlan {
        keep,
        removed,
        original: duration,
        result,
    }
}

/// Parses `silence_start: X` / `silence_end: Y` lines from ffmpeg's silencedetect output.
pub fn parse_silencedetect(stderr: &str, duration: f64) -> Vec<Range> {
    let mut out = Vec::new();
    let mut start: Option<f64> = None;
    for line in stderr.lines() {
        if let Some(i) = line.find("silence_start: ") {
            start = line[i + 15..]
                .split_whitespace()
                .next()
                .and_then(|v| v.parse().ok());
        } else if let Some(i) = line.find("silence_end: ") {
            let end: Option<f64> = line[i + 13..]
                .split_whitespace()
                .next()
                .and_then(|v| v.parse().ok());
            if let (Some(s), Some(e)) = (start.take(), end) {
                out.push(Range {
                    start: s.max(0.0),
                    end: e,
                });
            }
        }
    }
    if let Some(s) = start {
        out.push(Range {
            start: s,
            end: duration,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(start: f64, end: f64, text: &str) -> Word {
        Word {
            start,
            end,
            text: text.into(),
        }
    }

    #[test]
    fn detects_fillers_case_and_punctuation() {
        assert!(is_filler(" Um,"));
        assert!(is_filler("uh."));
        assert!(!is_filler("umbrella"));
        assert!(!is_filler(""));
    }

    #[test]
    fn removes_fillers_and_long_silences_only() {
        let words = vec![
            w(0.0, 0.5, "Hello"),
            w(0.6, 0.9, "um"),
            w(1.0, 1.4, "world"),
        ];
        let silences = vec![
            Range {
                start: 2.0,
                end: 4.0,
            },
            Range {
                start: 5.0,
                end: 5.3,
            },
        ];
        let p = plan(6.0, &words, &silences, &CutOptions::default());
        assert_eq!(p.removed.len(), 2, "short silence is kept");
        assert_eq!(
            p.keep,
            vec![
                Range {
                    start: 0.0,
                    end: 0.6
                },
                Range {
                    start: 0.9,
                    end: 2.2
                },
                Range {
                    start: 3.8,
                    end: 6.0
                },
            ]
        );
        assert!((p.result - (0.6 + 1.3 + 2.2)).abs() < 1e-9);
    }

    #[test]
    fn overlapping_removals_merge() {
        let words = vec![w(1.0, 1.5, "uh")];
        let silences = vec![Range {
            start: 0.5,
            end: 2.5,
        }];
        let p = plan(
            3.0,
            &words,
            &silences,
            &CutOptions {
                pad: 0.0,
                ..Default::default()
            },
        );
        assert_eq!(
            p.keep,
            vec![
                Range {
                    start: 0.0,
                    end: 0.5
                },
                Range {
                    start: 2.5,
                    end: 3.0
                }
            ]
        );
    }

    #[test]
    fn nothing_to_remove_keeps_everything() {
        let p = plan(10.0, &[], &[], &CutOptions::default());
        assert_eq!(
            p.keep,
            vec![Range {
                start: 0.0,
                end: 10.0
            }]
        );
    }

    #[test]
    fn parses_silencedetect() {
        let log = "[silencedetect @ 0x1] silence_start: 1.5\n[silencedetect @ 0x1] silence_end: 3.25 | silence_duration: 1.75\n[silencedetect @ 0x1] silence_start: 9\n";
        assert_eq!(
            parse_silencedetect(log, 10.0),
            vec![
                Range {
                    start: 1.5,
                    end: 3.25
                },
                Range {
                    start: 9.0,
                    end: 10.0
                }
            ]
        );
    }
}

/// Sentences as (start, end) spans, split where a word ends in . ? or ! or after a long pause.
pub fn sentences(words: &[Word]) -> Vec<Range> {
    let mut out = Vec::new();
    let mut start: Option<f64> = None;
    for (i, w) in words.iter().enumerate() {
        let s = *start.get_or_insert(w.start);
        let ends = w.text.trim_end().ends_with(['.', '?', '!']);
        let pause = words.get(i + 1).is_some_and(|n| n.start - w.end > 1.0);
        if ends || pause || i + 1 == words.len() {
            out.push(Range {
                start: s,
                end: w.end,
            });
            start = None;
        }
    }
    out
}

/// Moves a range's edges to the nearest sentence start and end.
pub fn snap_to_sentences(r: Range, sents: &[Range]) -> Range {
    let near = |t: f64, pick: fn(&Range) -> f64| {
        sents
            .iter()
            .map(pick)
            .min_by(|a, b| (a - t).abs().total_cmp(&(b - t).abs()))
            .unwrap_or(t)
    };
    let start = near(r.start, |s| s.start);
    let end = near(r.end, |s| s.end);
    if end > start { Range { start, end } } else { r }
}

#[cfg(test)]
mod sentence_tests {
    use super::*;

    #[test]
    fn splits_and_snaps_sentences() {
        let w = |s: f64, e: f64, t: &str| Word {
            start: s,
            end: e,
            text: t.into(),
        };
        let words = vec![
            w(0.0, 0.5, "Hi"),
            w(0.5, 1.0, "there."),
            w(1.2, 1.6, "Next"),
            w(1.6, 2.0, "part!"),
            w(4.0, 4.5, "Late"),
        ];
        let s = sentences(&words);
        assert_eq!(
            s,
            vec![
                Range {
                    start: 0.0,
                    end: 1.0
                },
                Range {
                    start: 1.2,
                    end: 2.0
                },
                Range {
                    start: 4.0,
                    end: 4.5
                }
            ]
        );
        assert_eq!(
            snap_to_sentences(
                Range {
                    start: 0.3,
                    end: 1.9
                },
                &s
            ),
            Range {
                start: 0.0,
                end: 2.0
            }
        );
    }
}
