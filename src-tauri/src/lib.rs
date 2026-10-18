pub mod agent;
pub mod ai;
pub mod brand;
pub mod captions;
pub mod capture;
mod commands;
pub mod cuts;
pub mod edits;
pub mod export;
pub mod importer;
pub mod library;
pub mod media;
pub mod server;
pub mod share;
pub mod shorter;
pub mod shorts;
pub mod sidecars;
pub mod transcribe;
pub mod voice;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

use brand::BRAND;

pub struct AppState {
    pub recorder: capture::Capture,
    pub tunnel: share::Tunnel,
    pub port: u16,
}

/// Places and shows (or hides) the recording controls and the camera bubble.
pub fn show_overlays(app: &AppHandle, hud: bool, bubble: bool) {
    let monitor = app.primary_monitor().ok().flatten();
    for (label, show) in [("hud", hud), ("bubble", bubble)] {
        let Some(w) = app.get_webview_window(label) else {
            continue;
        };
        if !show {
            let _ = w.hide();
            continue;
        }
        if let (Some(m), Ok(size)) = (&monitor, w.outer_size()) {
            let (mw, mh) = (m.size().width as i32, m.size().height as i32);
            let pad = (24.0 * m.scale_factor()) as i32;
            let pos = if label == "hud" {
                PhysicalPosition::new(
                    (mw - size.width as i32) / 2,
                    mh - size.height as i32 - pad * 3,
                )
            } else {
                PhysicalPosition::new(pad, mh - size.height as i32 - pad * 3)
            };
            let _ = w.set_position(pos);
        }
        let _ = w.show();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let port = tauri::async_runtime::block_on(server::start()).expect("start local server");
    let toggle = Shortcut::new(Some(Modifiers::ALT | Modifiers::SHIFT), Code::KeyR);

    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, sc, ev| {
                    if sc == &toggle && ev.state() == ShortcutState::Pressed {
                        let _ = app.emit("toggle-recording", ());
                    }
                })
                .build(),
        )
        .manage(AppState {
            recorder: Default::default(),
            tunnel: Default::default(),
            port,
        })
        .setup(move |app| {
            app.global_shortcut().register(toggle)?;
            let start = MenuItem::with_id(
                app,
                "start",
                "Start or stop recording (Alt+Shift+R)",
                true,
                None::<&str>,
            )?;
            let open = MenuItem::with_id(
                app,
                "open",
                format!("Open {}", BRAND.name),
                true,
                None::<&str>,
            )?;
            let quit = MenuItem::with_id(
                app,
                "quit",
                format!("Quit {}", BRAND.name),
                true,
                None::<&str>,
            )?;
            let menu = Menu::with_items(app, &[&start, &open, &quit])?;
            TrayIconBuilder::new()
                .icon(app.default_window_icon().cloned().expect("app icon"))
                .tooltip(&BRAND.name)
                .menu(&menu)
                .on_menu_event(|app, ev| match ev.id.as_ref() {
                    "start" => {
                        let _ = app.emit("toggle-recording", ());
                    }
                    "open" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;
            Ok(())
        })
        .on_window_event(|w, ev| {
            // Closing the library window keeps the app in the menu bar so shared links keep working.
            if let tauri::WindowEvent::CloseRequested { api, .. } = ev
                && w.label() == "main"
            {
                api.prevent_close();
                let _ = w.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::brand,
            commands::server_port,
            commands::list_videos,
            commands::get_video,
            commands::rename_video,
            commands::delete_video,
            commands::reveal_video,
            commands::devices,
            commands::start_recording,
            commands::pause_recording,
            commands::resume_recording,
            commands::recording_state,
            commands::stop_recording,
            commands::cancel_recording,
            commands::show_overlays,
            commands::hide_main,
            commands::transcribe_video,
            commands::generate_insights,
            commands::ask_video,
            commands::plan_cuts,
            commands::apply_cuts,
            commands::trim_video,
            commands::restore_original,
            commands::export_video,
            commands::set_public_questions,
            commands::share_start,
            commands::share_stop,
            commands::share_status,
            commands::get_settings,
            commands::save_settings,
            commands::ai_status,
            commands::test_claude,
            commands::tools_status,
            commands::open_privacy_settings,
            commands::set_voice,
            commands::voice_stats,
            commands::voice_preview,
            commands::plan_edit,
            commands::apply_plan,
            commands::undo_edit,
            commands::plan_shorter,
            commands::render_shorter,
            commands::translate_captions,
            commands::caption_languages,
            commands::forget_api_key,
            commands::install_model_file,
            commands::update_youtube_importer,
            commands::import_file,
            commands::import_youtube,
            commands::get_shorts,
            commands::suggest_shorts,
            commands::reroll_short,
            commands::update_short,
            commands::nudge_short,
            commands::render_shorts,
            commands::reveal_path,
        ])
        .build(tauri::generate_context!())
        .expect("error while building the app")
        .run(|app, event| {
            // Quitting stops the public link, so no tunnel outlives the app.
            if let tauri::RunEvent::Exit = event {
                tauri::async_runtime::block_on(app.state::<AppState>().tunnel.stop());
            }
        });
}
