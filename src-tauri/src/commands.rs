//! Tauri commands called from the UI. Errors become plain strings the UI can show.
use std::path::PathBuf;

use serde::Serialize;
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, State};

use crate::ai;
use crate::brand::{BRAND, Brand};
use crate::capture::{self, Devices, RecState, StartOptions};
use crate::cuts::{self, CutOptions, CutPlan};
use crate::library::{self, Video};
use crate::{AppState, media, transcribe, voice};

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
        "can_undo": crate::edits::can_undo(&id),
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
    let bubble = bubble_rect(&app);
    let mut res = state.recorder.stop().await.map_err(e);
    if let (Ok(v), Some(b)) = (res.as_mut(), bubble) {
        v.bubble = Some(b);
        let _ = library::save(v);
    }
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

/// Where the camera bubble sits on the screen, as fractions, so Shorts can crop the camera.
fn bubble_rect(app: &AppHandle) -> Option<(f64, f64, f64, f64)> {
    let w = app.get_webview_window("bubble")?;
    if !w.is_visible().ok()? {
        return None;
    }
    let (pos, size) = (w.outer_position().ok()?, w.outer_size().ok()?);
    let m = w.current_monitor().ok()??;
    // The circle sits inside a transparent margin (room for its shadow).
    let pad = (crate::BUBBLE_PAD * m.scale_factor()) as i32;
    let pos = PhysicalPosition::new(pos.x + pad, pos.y + pad);
    let size = PhysicalSize::new(
        size.width.saturating_sub(2 * pad as u32),
        size.height.saturating_sub(2 * pad as u32),
    );
    let (mw, mh) = (m.size().width as f64, m.size().height as f64);
    let (mx, my) = (m.position().x as f64, m.position().y as f64);
    Some((
        ((pos.x as f64 - mx) / mw).clamp(0.0, 1.0),
        ((pos.y as f64 - my) / mh).clamp(0.0, 1.0),
        (size.width as f64 / mw).min(1.0),
        (size.height as f64 / mh).min(1.0),
    ))
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
pub fn hud_layout(app: AppHandle, mode: String) {
    crate::hud_layout(&app, &mode);
}

#[tauri::command]
pub fn bubble_size(app: AppHandle, diameter: f64) {
    crate::bubble_size(&app, diameter);
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
    let (handle, vid) = (app.clone(), id.clone());
    let progress: transcribe::Progress =
        std::sync::Arc::new(move |stage: &str, done: u64, total: u64| {
            let _ = handle.emit(
                "progress",
                Progress {
                    id: vid.clone(),
                    stage: stage.into(),
                    done,
                    total,
                },
            );
        });
    progress("transcribing", 0, 0);
    let t = transcribe::transcribe(&id, progress.clone())
        .await
        .map_err(e)?;
    let mut v = library::load(&id).map_err(e)?;
    v.has_transcript = !t.words.is_empty();
    library::save(&v).map_err(e)?;
    progress("done", 0, 0);
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
    crate::voice::render(id, &v.voice).await?;
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

#[tauri::command]
pub async fn export_video(id: String, kind: String) -> Res<Value> {
    let (out, note) = crate::export::run(&id, &kind).await.map_err(e)?;
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
pub fn get_settings() -> Value {
    let s = ai::load_settings();
    // Never send the key to the UI, only whether one is saved.
    json!({
        "provider": s.provider, "api_key": if ai::api_key_saved() { Some("saved") } else { None },
        "voice_default": s.voice_default, "whisper_model": s.whisper_model, "onboarded": s.onboarded,
    })
}

#[tauri::command]
pub fn save_settings(
    provider: ai::Provider,
    api_key: Option<String>,
    voice_default: Option<voice::Voice>,
    whisper_model: Option<transcribe::Model>,
    onboarded: Option<bool>,
) -> Res<()> {
    let mut s = ai::load_settings();
    s.provider = provider;
    if let Some(v) = voice_default {
        s.voice_default = v.clamped();
    }
    if let Some(m) = whisper_model {
        s.whisper_model = m;
    }
    if let Some(o) = onboarded {
        s.onboarded = o;
    }
    if let Some(k) = api_key {
        ai::set_api_key(Some(&k)).map_err(e)?;
    }
    ai::save_settings(&s).map_err(e)
}

#[tauri::command]
pub fn forget_api_key() -> Res<()> {
    ai::set_api_key(None).map_err(e)
}

#[tauri::command]
pub async fn install_model_file(path: String, model: transcribe::Model) -> Res<()> {
    transcribe::install_model_from(std::path::Path::new(&path), model)
        .await
        .map_err(e)
}

#[tauri::command]
pub async fn update_youtube_importer() -> Res<String> {
    crate::sidecars::update_ytdlp().await.map_err(e)
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
    let m = transcribe::selected();
    json!({
        "ffmpeg": media::available("ffmpeg") && media::available("ffprobe"),
        "captions_burn": media::can_burn().await,
        "model": transcribe::model_ready(m),
        "model_name": m,
        "model_mb": m.size_mb(),
        "cloudflared": media::available("cloudflared"),
        "ytdlp": media::available("yt-dlp"),
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

#[tauri::command]
pub async fn set_voice(id: String, voice: voice::Voice) -> Res<Video> {
    let mut v = library::load(&id).map_err(e)?;
    v.voice = voice.clamped();
    voice::render(&id, &v.voice).await.map_err(e)?;
    library::save(&v).map_err(e)?;
    Ok(v)
}

#[tauri::command]
pub async fn voice_stats(id: String, voice: voice::Voice) -> Res<voice::Stats> {
    voice::stats(&id, &voice.clamped()).await.map_err(e)
}

/// Renders a 10 second clip with the voice applied; the UI plays it from /preview.
#[tauri::command]
pub async fn voice_preview(id: String, voice: voice::Voice, start: f64) -> Res<()> {
    voice::preview(&id, &voice.clamped(), start)
        .await
        .map(|_| ())
        .map_err(e)
}

#[tauri::command]
pub async fn plan_edit(id: String, prompt: String) -> Res<Value> {
    let v = library::load(&id).map_err(e)?;
    let t = library::load_transcript(&id);
    let plan = crate::edits::plan_from_prompt(&v, t.as_ref(), &prompt)
        .await
        .map_err(e)?;
    Ok(json!({ "plan": plan, "lines": crate::edits::describe(&plan, v.duration) }))
}

#[tauri::command]
pub async fn apply_plan(id: String, plan: Value) -> Res<Video> {
    let v = library::load(&id).map_err(e)?;
    let plan = crate::edits::validate(&plan, v.duration).map_err(e)?;
    crate::edits::apply(&id, &plan).await.map_err(e)
}

#[tauri::command]
pub async fn undo_edit(id: String) -> Res<Video> {
    crate::edits::undo(&id).await.map_err(e)
}

#[tauri::command]
pub async fn plan_shorter(id: String, target: f64) -> Res<crate::shorter::ShortPlan> {
    let v = library::load(&id).map_err(e)?;
    let t = library::load_transcript(&id).ok_or("Transcribe the video first.")?;
    crate::shorter::plan(&v, &t, target).await.map_err(e)
}

#[tauri::command]
pub async fn render_shorter(id: String, ranges: Vec<crate::cuts::Range>) -> Res<Video> {
    let v = library::load(&id).map_err(e)?;
    crate::shorter::render(&v, &ranges).await.map_err(e)
}

#[tauri::command]
pub async fn translate_captions(id: String, lang: String) -> Res<Video> {
    crate::captions::translate(&id, &lang).await.map_err(e)?;
    library::load(&id).map_err(e)
}

#[tauri::command]
pub fn caption_languages() -> Vec<(String, String)> {
    crate::captions::LANGUAGES
        .iter()
        .map(|(c, n)| (c.to_string(), n.to_string()))
        .collect()
}

#[tauri::command]
pub async fn import_file(path: String) -> Res<Video> {
    crate::importer::file(std::path::Path::new(path.trim()))
        .await
        .map_err(e)
}

#[tauri::command]
pub async fn import_youtube(app: AppHandle, url: String) -> Res<Video> {
    let v = crate::importer::youtube(&url, |p| {
        let _ = app.emit("import-progress", p);
    })
    .await
    .map_err(e)?;
    Ok(v)
}

#[tauri::command]
pub fn get_shorts(id: String) -> crate::shorts::ShortsFile {
    crate::shorts::load(&id)
}

#[tauri::command]
pub async fn suggest_shorts(id: String, limits: crate::shorts::Limits) -> Res<Value> {
    let v = library::load(&id).map_err(e)?;
    let t = library::load_transcript(&id).ok_or("Transcribe the video first.")?;
    let (clips, rejected) = crate::shorts::suggest(&v, &t, &limits, &[])
        .await
        .map_err(e)?;
    crate::shorts::save(
        &id,
        &crate::shorts::ShortsFile {
            limits: Some(limits),
            clips: clips.clone(),
            rendered: vec![],
        },
    )
    .map_err(e)?;
    Ok(json!({ "clips": clips, "rejected": rejected }))
}

#[tauri::command]
pub async fn reroll_short(id: String, clip_id: String) -> Res<crate::shorts::ShortsFile> {
    let v = library::load(&id).map_err(e)?;
    let t = library::load_transcript(&id).ok_or("Transcribe the video first.")?;
    let mut f = crate::shorts::load(&id);
    let limits = crate::shorts::Limits {
        count: 1,
        ..f.limits.unwrap_or_default()
    };
    let avoid: Vec<crate::cuts::Range> = f
        .clips
        .iter()
        .map(|c| crate::cuts::Range {
            start: c.start,
            end: c.end,
        })
        .collect();
    let (mut new, _) = crate::shorts::suggest(&v, &t, &limits, &avoid)
        .await
        .map_err(e)?;
    let mut replacement = new.pop().ok_or("Claude found no other clip that fits.")?;
    replacement.id = clip_id.clone();
    let slot = f
        .clips
        .iter_mut()
        .find(|c| c.id == clip_id)
        .ok_or("no such clip")?;
    *slot = replacement;
    crate::shorts::save(&id, &f).map_err(e)?;
    Ok(f)
}

#[tauri::command]
pub fn update_short(id: String, clip: crate::shorts::Clip) -> Res<crate::shorts::ShortsFile> {
    let mut f = crate::shorts::load(&id);
    let slot = f
        .clips
        .iter_mut()
        .find(|c| c.id == clip.id)
        .ok_or("no such clip")?;
    slot.title = clip.title.trim().chars().take(100).collect();
    slot.selected = clip.selected;
    crate::shorts::save(&id, &f).map_err(e)?;
    Ok(f)
}

#[tauri::command]
pub fn nudge_short(
    id: String,
    clip_id: String,
    edge: String,
    delta: f64,
    to_sentence: bool,
) -> Res<crate::shorts::ShortsFile> {
    let v = library::load(&id).map_err(e)?;
    let sents = library::load_transcript(&id)
        .map(|t| crate::cuts::sentences(&t.words))
        .unwrap_or_default();
    let mut f = crate::shorts::load(&id);
    let c = f
        .clips
        .iter_mut()
        .find(|c| c.id == clip_id)
        .ok_or("no such clip")?;
    crate::shorts::nudge(c, &edge, delta, to_sentence, &sents, v.duration);
    crate::shorts::save(&id, &f).map_err(e)?;
    Ok(f)
}

#[tauri::command]
pub async fn render_shorts(
    id: String,
    clip_ids: Vec<String>,
    framing: crate::shorts::Framing,
) -> Res<Value> {
    let v = library::load(&id).map_err(e)?;
    let (files, note) = crate::shorts::render(&v, &clip_ids, framing)
        .await
        .map_err(e)?;
    Ok(json!({ "files": files, "note": note }))
}

#[tauri::command]
pub fn reveal_path(path: String) -> Res<()> {
    let p = std::path::PathBuf::from(&path);
    if !p.starts_with(library::root()) {
        return Err("only files in the library can be shown".into());
    }
    std::process::Command::new("open")
        .arg("-R")
        .arg(p)
        .spawn()
        .map_err(e)?;
    Ok(())
}
