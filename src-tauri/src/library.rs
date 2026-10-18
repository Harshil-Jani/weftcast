//! The local video library: one folder per recording with the video, metadata and transcript.
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::brand::BRAND;
use crate::cuts::Word;

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Chapter {
    pub start: f64,
    pub title: String,
}

/// A comment pinned to a moment in the video.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Note {
    pub time: f64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Transcript {
    pub words: Vec<Word>,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Video {
    pub id: String,
    /// Unguessable id used in every URL, so a public link exposes only the videos you share.
    pub share_id: String,
    pub title: String,
    pub created_at: String,
    pub duration: f64,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub chapters: Vec<Chapter>,
    #[serde(default)]
    pub has_transcript: bool,
    /// Lets viewers of the public link ask questions (each one spends the owner's Claude usage).
    #[serde(default)]
    pub public_questions: bool,
    #[serde(default)]
    pub edited: bool,
    /// Voice loudness. Applied to a derived file; the raw audio is kept.
    #[serde(default)]
    pub voice: crate::voice::Voice,
    #[serde(default)]
    pub notes: Vec<Note>,
    /// Extra caption languages (ISO codes) saved as captions.<lang>.vtt.
    #[serde(default)]
    pub caption_langs: Vec<String>,
}

pub fn root() -> PathBuf {
    let base = dirs::video_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(std::env::temp_dir);
    base.join(&BRAND.name)
}

pub fn dir(id: &str) -> PathBuf {
    root().join(id)
}
pub fn video_path(id: &str) -> PathBuf {
    dir(id).join("video.mp4")
}
pub fn original_path(id: &str) -> PathBuf {
    dir(id).join("original.mp4")
}
/// The raw edit with the voice settings applied (video stream copied). Missing when voice is off.
pub fn voiced_path(id: &str) -> PathBuf {
    dir(id).join("voiced.mp4")
}
pub fn preview_path(id: &str) -> PathBuf {
    dir(id).join("preview.mp4")
}
/// What viewers and exports get: the voiced file when there is one, else the raw edit.
pub fn playable_path(id: &str) -> PathBuf {
    let v = voiced_path(id);
    if exists(&v) { v } else { video_path(id) }
}
pub fn thumb_path(id: &str) -> PathBuf {
    dir(id).join("thumb.jpg")
}
fn meta_path(id: &str) -> PathBuf {
    dir(id).join("meta.json")
}
fn transcript_path(id: &str) -> PathBuf {
    dir(id).join("transcript.json")
}

pub fn new_id() -> String {
    chrono::Local::now().format("%Y%m%d-%H%M%S").to_string()
}

pub fn new_share_id() -> String {
    use rand::Rng;
    let mut rng = rand::rng();
    (0..20)
        .map(|_| char::from(b"abcdefghijkmnpqrstuvwxyz23456789"[rng.random_range(0..32)]))
        .collect()
}

pub fn save(v: &Video) -> Result<()> {
    std::fs::create_dir_all(dir(&v.id))?;
    std::fs::write(meta_path(&v.id), serde_json::to_vec_pretty(v)?)?;
    Ok(())
}

pub fn load(id: &str) -> Result<Video> {
    let bytes = std::fs::read(meta_path(id)).with_context(|| format!("no video {id}"))?;
    Ok(serde_json::from_slice(&bytes)?)
}

pub fn list() -> Vec<Video> {
    let mut out: Vec<Video> = std::fs::read_dir(root())
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| load(&e.file_name().to_string_lossy()).ok())
        .collect();
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    out
}

pub fn by_share(share_id: &str) -> Option<Video> {
    list().into_iter().find(|v| v.share_id == share_id)
}

pub fn delete(id: &str) -> Result<()> {
    let d = dir(id);
    anyhow::ensure!(
        d.starts_with(root()) && d != root(),
        "refusing to delete outside the library"
    );
    std::fs::remove_dir_all(d)?;
    Ok(())
}

pub fn save_transcript(id: &str, t: &Transcript) -> Result<()> {
    std::fs::write(transcript_path(id), serde_json::to_vec(t)?)?;
    Ok(())
}

pub fn load_transcript(id: &str) -> Option<Transcript> {
    serde_json::from_slice(&std::fs::read(transcript_path(id)).ok()?).ok()
}

pub fn clear_transcript(id: &str) {
    let _ = std::fs::remove_file(transcript_path(id));
}

pub fn exists(p: &Path) -> bool {
    p.metadata().map(|m| m.len() > 0).unwrap_or(false)
}

/// Caption cues: words grouped into short lines for the player and for SRT/VTT.
pub fn cues(words: &[Word]) -> Vec<(f64, f64, String)> {
    let mut cues = Vec::new();
    let mut cur: Vec<&Word> = Vec::new();
    for w in words {
        let text_len: usize = cur.iter().map(|w| w.text.trim().len() + 1).sum();
        let gap = cur.last().map(|l| w.start - l.end).unwrap_or(0.0);
        if !cur.is_empty() && (text_len + w.text.trim().len() > 42 || gap > 0.8) {
            cues.push(flush(&cur));
            cur.clear();
        }
        cur.push(w);
    }
    if !cur.is_empty() {
        cues.push(flush(&cur));
    }
    cues
}

fn flush(ws: &[&Word]) -> (f64, f64, String) {
    let text = ws
        .iter()
        .map(|w| w.text.trim())
        .collect::<Vec<_>>()
        .join(" ");
    (ws[0].start, ws[ws.len() - 1].end, text)
}

fn stamp(t: f64, sep: char) -> String {
    let ms = (t.max(0.0) * 1000.0).round() as u64;
    format!(
        "{:02}:{:02}:{:02}{}{:03}",
        ms / 3_600_000,
        ms / 60_000 % 60,
        ms / 1000 % 60,
        sep,
        ms % 1000
    )
}

pub fn to_vtt(words: &[Word]) -> String {
    let mut s = String::from("WEBVTT\n\n");
    for (a, b, t) in cues(words) {
        s += &format!("{} --> {}\n{}\n\n", stamp(a, '.'), stamp(b, '.'), t);
    }
    s
}

pub fn to_srt(words: &[Word]) -> String {
    cues(words)
        .into_iter()
        .enumerate()
        .map(|(i, (a, b, t))| {
            format!(
                "{}\n{} --> {}\n{}\n\n",
                i + 1,
                stamp(a, ','),
                stamp(b, ','),
                t
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captions_split_on_length_and_gaps() {
        let words: Vec<Word> = ["Hello", "there", "friend"]
            .iter()
            .enumerate()
            .map(|(i, t)| Word {
                start: i as f64 * 0.5,
                end: i as f64 * 0.5 + 0.4,
                text: t.to_string(),
            })
            .chain([Word {
                start: 5.0,
                end: 5.5,
                text: "Later".into(),
            }])
            .collect();
        let c = cues(&words);
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].2, "Hello there friend");
        assert!(to_srt(&words).starts_with("1\n00:00:00,000 --> 00:00:01,400\nHello there friend"));
        assert!(to_vtt(&words).contains("00:00:05.000 --> 00:00:05.500\nLater"));
    }

    #[test]
    fn share_ids_are_long_and_unique() {
        let a = new_share_id();
        assert_eq!(a.len(), 20);
        assert_ne!(a, new_share_id());
    }
}
