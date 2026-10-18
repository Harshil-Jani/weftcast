//! Voice loudness: a fixed gain and/or "Auto level" (gentle compressor + two-pass loudnorm to
//! -16 LUFS, -1.5 dBTP). Non-destructive: `video.mp4` keeps the raw audio and `voiced.mp4` is
//! derived from it with the video stream copied, so changing or turning off the voice is cheap.
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::library;
use crate::media;

pub const TARGET_LUFS: f64 = -16.0;
pub const TARGET_TP: f64 = -1.5;
/// Above this true peak the UI warns about clipping.
pub const CLIP_TP: f64 = -1.0;
const COMPRESSOR: &str = "acompressor=threshold=-20dB:ratio=3:attack=5:release=250:makeup=2";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq)]
pub struct Voice {
    /// Fixed gain in dB, -12 to +12.
    #[serde(default)]
    pub gain_db: f64,
    /// Compressor + loudness normalisation.
    #[serde(default)]
    pub auto: bool,
}

impl Voice {
    pub fn clamped(self) -> Self {
        Self {
            gain_db: self.gain_db.clamp(-12.0, 12.0).round(),
            ..self
        }
    }
    pub fn is_off(&self) -> bool {
        !self.auto && self.gain_db.abs() < 0.5
    }
}

/// First-pass loudnorm analysis, fed back into the second pass.
#[derive(Debug, Clone, Deserialize)]
pub struct Analysis {
    pub input_i: String,
    pub input_tp: String,
    pub input_lra: String,
    pub input_thresh: String,
    pub target_offset: String,
}

impl Analysis {
    pub fn lufs(&self) -> f64 {
        self.input_i.parse().unwrap_or(f64::NEG_INFINITY)
    }
    pub fn true_peak(&self) -> f64 {
        self.input_tp.parse().unwrap_or(f64::NEG_INFINITY)
    }
}

/// The audio filter chain for `voice`. Auto level needs the first-pass `analysis`.
/// Order: compressor, loudnorm, then the fixed gain as a trim on top.
pub fn chain(voice: &Voice, analysis: Option<&Analysis>) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    if voice.auto {
        parts.push(COMPRESSOR.into());
        parts.push(match analysis {
            Some(a) => format!(
                "loudnorm=I={TARGET_LUFS}:TP={TARGET_TP}:LRA=11:measured_I={}:measured_TP={}:measured_LRA={}:measured_thresh={}:offset={}:linear=true",
                a.input_i, a.input_tp, a.input_lra, a.input_thresh, a.target_offset
            ),
            // Single pass (used only if analysis failed): still lands near the target.
            None => format!("loudnorm=I={TARGET_LUFS}:TP={TARGET_TP}:LRA=11"),
        });
    }
    if voice.gain_db.abs() >= 0.5 {
        parts.push(format!("volume={:.0}dB", voice.gain_db));
    }
    (!parts.is_empty()).then(|| parts.join(","))
}

/// Pulls the JSON block that loudnorm prints with `print_format=json`.
pub fn parse_analysis(stderr: &str) -> Option<Analysis> {
    let start = stderr.rfind("[Parsed_loudnorm")?;
    let rest = &stderr[start..];
    let (a, b) = (rest.find('{')?, rest.rfind('}')?);
    serde_json::from_str(&rest[a..=b]).ok()
}

/// Loudness of `file` after `pre` (an optional filter chain), measured by loudnorm's first pass.
/// `window` limits it to `(start, seconds)` for previews.
pub async fn analyze(
    file: &Path,
    pre: Option<&str>,
    window: Option<(f64, f64)>,
) -> Result<Analysis> {
    let af = match pre {
        Some(p) => format!("{p},loudnorm=I={TARGET_LUFS}:TP={TARGET_TP}:LRA=11:print_format=json"),
        None => format!("loudnorm=I={TARGET_LUFS}:TP={TARGET_TP}:LRA=11:print_format=json"),
    };
    let (ss, t) = window
        .map(|(s, d)| (format!("{s:.3}"), format!("{d:.3}")))
        .unwrap_or_default();
    let mut args = vec!["-hide_banner", "-nostats"];
    if window.is_some() {
        args.extend(["-ss", &ss, "-t", &t]);
    }
    let f = file.to_string_lossy();
    args.extend(["-i", &f, "-vn", "-af", &af, "-f", "null", "-"]);
    let log = media::run("ffmpeg", &args).await?;
    parse_analysis(&log).context("could not measure the loudness (does the video have audio?)")
}

async fn full_chain(
    file: &Path,
    voice: &Voice,
    window: Option<(f64, f64)>,
) -> Result<Option<String>> {
    let analysis = if voice.auto {
        analyze(file, Some(COMPRESSOR), window).await.ok()
    } else {
        None
    };
    Ok(chain(voice, analysis.as_ref()))
}

/// Renders `voiced.mp4` from the raw `video.mp4` (video stream copied), or removes it when off.
pub async fn render(id: &str, voice: &Voice) -> Result<()> {
    let out = library::voiced_path(id);
    let _ = tokio::fs::remove_file(&out).await;
    if voice.is_off() {
        return Ok(());
    }
    let src = library::video_path(id);
    let af = full_chain(&src, voice, None)
        .await?
        .context("nothing to apply")?;
    let tmp = library::dir(id).join("voiced.tmp.mp4");
    media::audio_only(&src, &tmp, &af, None).await?;
    tokio::fs::rename(&tmp, &out).await?;
    Ok(())
}

/// A short clip from `start` with the voice applied, for hearing Auto level before applying it.
pub async fn preview(id: &str, voice: &Voice, start: f64) -> Result<std::path::PathBuf> {
    let src = library::video_path(id);
    let window = (start.max(0.0), 10.0);
    let out = library::preview_path(id);
    let af = full_chain(&src, voice, Some(window))
        .await?
        .unwrap_or_else(|| "anull".into());
    media::audio_only(&src, &out, &af, Some(window)).await?;
    Ok(out)
}

#[derive(Debug, Serialize)]
pub struct Level {
    pub lufs: f64,
    pub true_peak: f64,
}

#[derive(Debug, Serialize)]
pub struct Stats {
    pub raw: Level,
    pub applied: Option<Level>,
    /// Raw true peak plus the fixed gain: what a gain-only change would hit.
    pub predicted_peak: f64,
    pub clips: bool,
}

pub async fn stats(id: &str, voice: &Voice) -> Result<Stats> {
    let raw = analyze(&library::video_path(id), None, None).await?;
    let applied = if library::exists(&library::voiced_path(id)) {
        analyze(&library::voiced_path(id), None, None).await.ok()
    } else {
        None
    };
    let base_peak = if voice.auto {
        TARGET_TP
    } else {
        raw.true_peak()
    };
    let predicted_peak = base_peak + voice.gain_db;
    let clips =
        predicted_peak > CLIP_TP || applied.as_ref().is_some_and(|a| a.true_peak() > CLIP_TP);
    Ok(Stats {
        raw: Level {
            lufs: raw.lufs(),
            true_peak: raw.true_peak(),
        },
        applied: applied.map(|a| Level {
            lufs: a.lufs(),
            true_peak: a.true_peak(),
        }),
        predicted_peak,
        clips,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cuts::Range;
    use crate::media::keep_filter;

    fn analysis() -> Analysis {
        Analysis {
            input_i: "-30.20".into(),
            input_tp: "-9.10".into(),
            input_lra: "4.30".into(),
            input_thresh: "-40.50".into(),
            target_offset: "0.30".into(),
        }
    }

    #[test]
    fn off_has_no_chain() {
        assert_eq!(chain(&Voice::default(), None), None);
        assert!(
            Voice {
                gain_db: 0.2,
                auto: false
            }
            .is_off()
        );
    }

    #[test]
    fn gain_only() {
        assert_eq!(
            chain(
                &Voice {
                    gain_db: 6.0,
                    auto: false
                },
                None
            )
            .unwrap(),
            "volume=6dB"
        );
        assert_eq!(
            chain(
                &Voice {
                    gain_db: -3.0,
                    auto: false
                },
                None
            )
            .unwrap(),
            "volume=-3dB"
        );
    }

    #[test]
    fn auto_only_is_two_pass() {
        let c = chain(
            &Voice {
                gain_db: 0.0,
                auto: true,
            },
            Some(&analysis()),
        )
        .unwrap();
        assert!(c.starts_with("acompressor="));
        assert!(c.contains("loudnorm=I=-16:TP=-1.5:LRA=11:measured_I=-30.20:measured_TP=-9.10"));
        assert!(c.ends_with("offset=0.30:linear=true"));
    }

    #[test]
    fn auto_and_gain_puts_gain_last() {
        let c = chain(
            &Voice {
                gain_db: -2.0,
                auto: true,
            },
            Some(&analysis()),
        )
        .unwrap();
        let parts: Vec<&str> = c.split(',').collect();
        assert!(
            parts[0].starts_with("acompressor")
                && parts[1].starts_with("loudnorm")
                && parts[2] == "volume=-2dB"
        );
    }

    #[test]
    fn with_cuts_the_voice_runs_after_the_concat() {
        let v = chain(
            &Voice {
                gain_db: 4.0,
                auto: false,
            },
            None,
        );
        let g = keep_filter(
            &[
                Range {
                    start: 0.0,
                    end: 1.0,
                },
                Range {
                    start: 2.0,
                    end: 3.0,
                },
            ],
            v.as_deref(),
        );
        assert!(g.ends_with("[v0][a0][v1][a1]concat=n=2:v=1:a=1[v][acat];[acat]volume=4dB[a]"));
    }

    #[test]
    fn clamps_gain() {
        assert_eq!(
            Voice {
                gain_db: 20.4,
                auto: false
            }
            .clamped()
            .gain_db,
            12.0
        );
        assert_eq!(
            Voice {
                gain_db: -3.6,
                auto: false
            }
            .clamped()
            .gain_db,
            -4.0
        );
    }

    #[test]
    fn parses_loudnorm_json() {
        let log = "frame=...\n[Parsed_loudnorm_1 @ 0x1] \n{\n\t\"input_i\" : \"-27.61\",\n\t\"input_tp\" : \"-4.47\",\n\t\"input_lra\" : \"18.06\",\n\t\"input_thresh\" : \"-39.20\",\n\t\"output_i\" : \"-16.58\",\n\t\"target_offset\" : \"0.58\"\n}\n";
        let a = parse_analysis(log).unwrap();
        assert_eq!(a.lufs(), -27.61);
        assert_eq!(a.true_peak(), -4.47);
    }
}
