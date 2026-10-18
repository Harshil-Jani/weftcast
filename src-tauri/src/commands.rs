//! Tauri commands called from the UI. Errors become plain strings the UI can show.
use std::path::PathBuf;

use serde::Serialize;
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::ai::{self, Settings};
use crate::brand::{BRAND, Brand};
use crate::capture::{self, Devices, RecState, StartOptions};
use crate::cuts::{self, CutOptions, CutPlan};
use crate::library::{self, Video};
use crate::{AppState, media, share, transcribe};

type Res<T> = Result<T, String>;
fn e(err: impl std::fmt::Display) -> String {
    err.to_string()
}

#[tauri::command]
pub fn brand() -> Brand {
    BRAND.clone()
}

#[tauri::command]
pub fn server_port(state: State<AppState>) -> u16 {
    state.port
}

#[tauri::command]
pub fn list_videos() -> Vec<Video> {
    library::list()
}

#[tauri::command]
pub fn get_video(id: String) -> Res<Value> {
    let v = library::load(&id).map_err(e)?;
    let t = library::load_transcript(&id);
    Ok(json!({
        "video": v,
        "cues": t.as_ref().map(|t| library::cues(&t.words)).unwrap_or_default(),
        "has_original": library::exists(&library::original_path(&id)),
    }))
}

#[tauri::command]
pub fn rename_video(id: String, title: String) -> Res<Video> {
    let mut v = library::load(&id).map_err(e)?;
    let t = title.trim();
    if t.is_empty() {
        return Err("Title can't be empty".into());
    }
    v.title = t.chars().take(120).collect();
    library::save(&v).map_err(e)?;
    Ok(v)
}

#[tauri::command]
pub fn delete_video(id: String) -> Res<()> {
    library::delete(&id).map_err(e)
}

#[tauri::command]
pub fn reveal_video(id: String) -> Res<()> {
    std::process::Command::new("open")
        .arg("-R")
        .arg(library::video_path(&id))
        .spawn()
        .map_err(e)?;
    Ok(())
}

#[tauri::command]
pub async fn devices() -> Res<Devices> {
    capture::devices().await.map_err(e)
}

#[tauri::command]
pub async fn start_recording(
    app: AppHandle,
    state: State<'_, AppState>,
    opts: StartOptions,
) -> Res<String> {
    let id = state.recorder.start(opts).await.map_err(e)?;
    let _ = app.emit("recording-changed", true);
    Ok(id)
}

#[tauri::command]
pub async fn pause_recording(state: State<'_, AppState>) -> Res<()> {
    state.recorder.pause().await.map_err(e)
}

#[tauri::command]
pub async fn resume_recording(state: State<'_, AppState>) -> Res<()> {
    state.recorder.resume().await.map_err(e)
}

#[tauri::command]
pub async fn recording_state(state: State<'_, AppState>) -> Res<RecState> {
    Ok(state.recorder.state().await)
}

#[tauri::command]
pub async fn stop_recording(app: AppHandle, state: State<'_, AppState>) -> Res<Video> {
    let res = state.recorder.stop().await.map_err(e);
    crate::show_overlays(&app, false, false);
    let _ = app.emit("recording-changed", false);
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
    }
    if let Ok(v) = &res {
        let _ = app.emit("recording-saved", v.id.clone());
    }
    res
}

#[tauri::command]
pub async fn cancel_recording(app: AppHandle, state: State<'_, AppState>) -> Res<()> {
    state.recorder.cancel().await.map_err(e)?;
    crate::show_overlays(&app, false, false);
    let _ = app.emit("recording-changed", false);
    Ok(())
}

#[tauri::command]
pub fn show_overlays(app: AppHandle, hud: bool, bubble: bool) {
    crate::show_overlays(&app, hud, bubble);
}

#[tauri::command]
pub fn hide_main(app: AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.hide();
    }
}

#[derive(Serialize, Clone)]
struct Progress {
    id: String,
    stage: String,
    done: u64,
    total: u64,
}

#[tauri::command]
pub async fn transcribe_video(app: AppHandle, id: String) -> Res<Value> {
    let emit = |stage: &str, done: u64, total: u64| {
        let _ = app.emit(
            "progress",
            Progress {
                id: id.clone(),
                stage: stage.into(),
                done,
                total,
            },
        );
    };
    emit("transcribing", 0, 0);
    let t = transcribe::transcribe(&id, |d, t| emit("downloading speech model", d, t))
        .await
        .map_err(e)?;
    let mut v = library::load(&id).map_err(e)?;
    v.has_transcript = !t.words.is_empty();
    library::save(&v).map_err(e)?;
    emit("done", 0, 0);
    get_video(id)
}

#[tauri::command]
pub async fn generate_insights(id: String) -> Res<Video> {
    let mut v = library::load(&id).map_err(e)?;
    let t = library::load_transcript(&id).ok_or("Transcribe the video first.")?;
    if t.words.is_empty() {
        return Err("The transcript is empty, so there is nothing to summarize.".into());
    }
    let ins = ai::insights(&t, v.duration).await.map_err(e)?;
    v.title = ins.title;
    v.summary = Some(ins.summary);
    v.chapters = ins.chapters;
    library::save(&v).map_err(e)?;
    Ok(v)
}

#[tauri::command]
pub async fn ask_video(id: String, question: String) -> Res<ai::Answer> {
    let v = library::load(&id).map_err(e)?;
    let t = library::load_transcript(&id).ok_or("Transcribe the video first.")?;
    ai::ask(&t, &v.title, &question).await.map_err(e)
}

async fn cut_plan(id: &str, opts: &CutOptions) -> anyhow::Result<CutPlan> {
    let v = library::load(id)?;
    let words = library::load_transcript(id)
        .map(|t| t.words)
        .unwrap_or_default();
    if opts.remove_fillers && words.is_empty() {
        anyhow::bail!("Transcribe the video first so filler words can be found.");
    }
    let silences = if opts.remove_silences {
        cuts::parse_silencedetect(
            &media::silences(&library::video_path(id), -35, opts.min_silence).await?,
            v.duration,
        )
    } else {
        vec![]
    };
    Ok(cuts::plan(v.duration, &words, &silences, opts))
}

#[tauri::command]
pub async fn plan_cuts(id: String, opts: CutOptions) -> Res<CutPlan> {
    cut_plan(&id, &opts).await.map_err(e)
}

/// Re-encodes the video in place, keeping a one-time backup of the original.
async fn replace_video(
    id: &str,
    render: impl AsyncFnOnce(PathBuf, PathBuf) -> anyhow::Result<()>,
) -> anyhow::Result<Video> {
    let current = library::video_path(id);
    let original = library::original_path(id);
    if !library::exists(&original) {
        tokio::fs::copy(&current, &original).await?;
    }
    let tmp = library::dir(id).join("edit.mp4");
    render(current.clone(), tmp.clone()).await?;
    tokio::fs::rename(&tmp, &current).await?;
    after_edit(id).await
}

async fn after_edit(id: &str) -> anyhow::Result<Video> {
    let mut v = library::load(id)?;
    v.duration = media::duration(&library::video_path(id)).await?;
    v.edited = true;
    // Word timings no longer match the new cut.
    v.has_transcript = false;
    v.chapters.clear();
    library::clear_transcript(id);
    let _ = media::thumbnail(
        &library::video_path(id),
        &library::thumb_path(id),
        (v.duration * 0.1).min(2.0),
    )
    .await;
    library::save(&v)?;
    Ok(v)
}

#[tauri::command]
pub async fn apply_cuts(id: String, opts: CutOptions) -> Res<Video> {
    let plan = cut_plan(&id, &opts).await.map_err(e)?;
    if plan.removed.is_empty() {
        return Err("Nothing to remove.".into());
    }
    replace_video(&id, async |src, out| {
        media::render_keep(&src, &out, &plan.keep).await
    })
    .await
    .map_err(e)
}

#[tauri::command]
pub async fn trim_video(id: String, start: f64, end: f64) -> Res<Video> {
    let v = library::load(&id).map_err(e)?;
    let (s, en) = (start.max(0.0), end.min(v.duration));
    if en - s < 0.5 {
        return Err("Keep at least half a second.".into());
    }
    replace_video(&id, async |src, out| media::trim(&src, &out, s, en).await)
        .await
        .map_err(e)
}

#[tauri::command]
pub async fn restore_original(id: String) -> Res<Video> {
    let original = library::original_path(&id);
    if !library::exists(&original) {
        return Err("There is no original to restore.".into());
    }
    tokio::fs::copy(&original, library::video_path(&id))
        .await
        .map_err(e)?;
    let _ = tokio::fs::remove_file(&original).await;
    let mut v = after_edit(&id).await.map_err(e)?;
    v.edited = false;
    library::save(&v).map_err(e)?;
    Ok(v)
}

fn export_path(v: &Video, ext: &str) -> PathBuf {
    let safe: String = v
        .title
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == ' ' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    dirs::download_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("{}.{ext}", safe.trim()))
}

#[tauri::command]
pub async fn export_video(id: String, kind: String) -> Res<Value> {
    let v = library::load(&id).map_err(e)?;
    let words = library::load_transcript(&id)
        .map(|t| t.words)
        .unwrap_or_default();
    let src = library::video_path(&id);
    let mut note = None;
    let out = match kind.as_str() {
        "mp4" => {
            let out = export_path(&v, "mp4");
            tokio::fs::copy(&src, &out).await.map_err(e)?;
            out
        }
        "srt" | "vtt" => {
            if words.is_empty() {
                return Err("Transcribe the video first.".into());
            }
            let out = export_path(&v, &kind);
            let body = if kind == "srt" {
                library::to_srt(&words)
            } else {
                library::to_vtt(&words)
            };
            tokio::fs::write(&out, body).await.map_err(e)?;
            out
        }
        "captioned" => {
            if words.is_empty() {
                return Err("Transcribe the video first.".into());
            }
            let srt = library::dir(&id).join("captions.srt");
            tokio::fs::write(&srt, library::to_srt(&words))
                .await
                .map_err(e)?;
            let out = export_path(&v, "mp4");
            if media::can_burn().await {
                media::burn_captions(&src, &srt, &out).await.map_err(e)?;
            } else {
                media::embed_captions(&src, &srt, &out).await.map_err(e)?;
                note = Some("Your ffmpeg can't draw text, so captions were added as a track viewers can turn on (CC). For burned-in captions install an ffmpeg built with libass.".to_string());
            }
            out
        }
        _ => return Err("unknown export".into()),
    };
    let _ = std::process::Command::new("open")
        .arg("-R")
        .arg(&out)
        .spawn();
    Ok(json!({ "path": out, "note": note }))
}

#[tauri::command]
pub fn set_public_questions(id: String, on: bool) -> Res<Video> {
    let mut v = library::load(&id).map_err(e)?;
    v.public_questions = on;
    library::save(&v).map_err(e)?;
    Ok(v)
}

#[tauri::command]
pub async fn share_start(state: State<'_, AppState>) -> Res<String> {
    state.tunnel.start(state.port).await.map_err(e)
}

#[tauri::command]
pub async fn share_stop(state: State<'_, AppState>) -> Res<()> {
    state.tunnel.stop().await;
    Ok(())
}

#[tauri::command]
pub async fn share_status(state: State<'_, AppState>) -> Res<Option<String>> {
    Ok(state.tunnel.url().await)
}

#[tauri::command]
pub fn get_settings() -> Settings {
    let mut s = ai::load_settings();
    // Never send the key back to the UI, only whether one is saved.
    s.api_key = s.api_key.map(|_| "saved".into());
    s
}

#[tauri::command]
pub fn save_settings(provider: ai::Provider, api_key: Option<String>) -> Res<()> {
    let mut s = ai::load_settings();
    s.provider = provider;
    if let Some(k) = api_key {
        s.api_key = if k.trim().is_empty() {
            None
        } else {
            Some(k.trim().to_string())
        };
    }
    ai::save_settings(&s).map_err(e)
}

#[tauri::command]
pub async fn ai_status() -> ai::ProviderStatus {
    ai::status().await
}

#[tauri::command]
pub async fn test_claude() -> Res<String> {
    let schema = json!({"type": "object", "additionalProperties": false, "required": ["ok"], "properties": {"ok": {"type": "boolean"}}});
    let v = ai::ask_json("Reply with {\"ok\": true}.", "Health check.", &schema)
        .await
        .map_err(e)?;
    if v["ok"] == true {
        Ok("Claude is connected.".into())
    } else {
        Err(format!("Unexpected reply: {v}"))
    }
}

#[tauri::command]
pub async fn tools_status() -> Value {
    json!({
        "ffmpeg": media::tool("ffmpeg").exists(),
        "whisper": transcribe::cli_ready(),
        "model": transcribe::model_ready(),
        "cloudflared": share::installed(),
        "library": library::root(),
    })
}

#[tauri::command]
pub fn open_privacy_settings(pane: String) -> Res<()> {
    let anchor = match pane.as_str() {
        "camera" => "Privacy_Camera",
        "microphone" => "Privacy_Microphone",
        _ => "Privacy_ScreenCapture",
    };
    std::process::Command::new("open")
        .arg(format!(
            "x-apple.systempreferences:com.apple.preference.security?{anchor}"
        ))
        .spawn()
        .map_err(e)?;
    Ok(())
}
