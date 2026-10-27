pub mod agent;
pub mod ai;
pub mod airdrop;
pub mod airplay;
pub mod android;
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
pub mod pairing;
pub mod phone;
pub mod server;
pub mod share;
pub mod shorter;
pub mod shorts;
pub mod sidecars;
pub mod transcribe;
pub mod voice;

use tauri::menu::{AboutMetadata, Menu, MenuBuilder, MenuItem, MenuItemBuilder, SubmenuBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, Theme, Wry};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

use brand::BRAND;

pub struct AppState {
    pub recorder: capture::Capture,
    pub tunnel: share::Tunnel,
    pub port: u16,
    /// Who needs the tunnel: (Connect Phone, public share links). It stops when neither does.
    pub tunnel_users: std::sync::Mutex<(bool, bool)>,
}

/// Transparent margin around the camera circle, so its shadow is not clipped by the window.
pub const BUBBLE_PAD: f64 = 10.0;

/// Places and shows (or hides) the recording controls and the camera bubble.
pub fn show_overlays(app: &AppHandle, hud: bool, bubble: bool) {
    for (label, show) in [("hud", hud), ("bubble", bubble)] {
        let Some(w) = app.get_webview_window(label) else {
            continue;
        };
        if !show {
            let _ = w.hide();
            continue;
        }
        if !w.is_visible().unwrap_or(false) {
            place(&w, label == "hud", false);
        }
        let _ = w.show();
    }
}

/// Bottom centre for the controls, bottom left for the bubble, dead centre for the countdown.
fn place(w: &tauri::WebviewWindow, hud: bool, center: bool) {
    let (Some(m), Ok(size)) = (
        w.app_handle().primary_monitor().ok().flatten(),
        w.outer_size(),
    ) else {
        return;
    };
    let (mw, mh) = (m.size().width as i32, m.size().height as i32);
    let (ww, wh) = (size.width as i32, size.height as i32);
    let pad = (24.0 * m.scale_factor()) as i32;
    let pos = match (hud, center) {
        (_, true) => PhysicalPosition::new((mw - ww) / 2, (mh - wh) / 2),
        (true, _) => PhysicalPosition::new((mw - ww) / 2, mh - wh - pad * 3),
        (false, _) => PhysicalPosition::new(pad, mh - wh - pad * 3),
    };
    let _ = w.set_position(pos);
}

/// The controls window changes shape: the setup pill, the recording pill, the countdown.
pub fn hud_layout(app: &AppHandle, mode: &str) {
    let Some(w) = app.get_webview_window("hud") else {
        return;
    };
    let (width, height) = match mode {
        "countdown" => (220.0, 220.0),
        "recording" => (260.0, 60.0),
        "recording-phone" => (440.0, 60.0),
        "error" => (480.0, 60.0),
        "setup-phone" => (640.0, 60.0),
        _ => (600.0, 60.0),
    };
    let _ = w.set_size(LogicalSize::new(width, height));
    place(&w, true, mode == "countdown");
}

/// Small, medium or large camera circle; the bottom-left corner stays where the user put it.
pub fn bubble_size(app: &AppHandle, diameter: f64) {
    let Some(w) = app.get_webview_window("bubble") else {
        return;
    };
    let scale = w.scale_factor().unwrap_or(1.0);
    let side = diameter.clamp(100.0, 360.0) + BUBBLE_PAD * 2.0;
    if let (Ok(pos), Ok(old)) = (w.outer_position(), w.outer_size()) {
        let new_h = (side * scale) as i32;
        let _ = w.set_size(LogicalSize::new(side, side));
        let _ = w.set_position(PhysicalPosition::new(
            pos.x,
            pos.y + old.height as i32 - new_h,
        ));
    }
}

/// The macOS menu bar. Custom items are forwarded to the library window as a "menu" event.
fn menu_bar(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let item = |id: &str, text: &str, accel: &str| {
        MenuItemBuilder::with_id(id, text)
            .accelerator(accel)
            .build(app)
    };
    let app_menu = SubmenuBuilder::new(app, &BRAND.name)
        .about(Some(AboutMetadata {
            name: Some(BRAND.name.clone()),
            comments: Some(BRAND.tagline.clone()),
            website: Some(BRAND.repo.clone()),
            ..Default::default()
        }))
        .separator()
        .item(&item("settings", "Settings\u{2026}", "CmdOrCtrl+,")?)
        .separator()
        .services()
        .separator()
        .hide()
        .hide_others()
        .show_all()
        .separator()
        .quit()
        .build()?;
    let file = SubmenuBuilder::new(app, "File")
        .item(&item("new-recording", "New Recording", "CmdOrCtrl+N")?)
        .item(&item(
            "connect-phone",
            "Connect Phone\u{2026}",
            "Shift+CmdOrCtrl+P",
        )?)
        .item(&item("import", "Import Video\u{2026}", "CmdOrCtrl+O")?)
        .separator()
        .item(&item("export", "Export\u{2026}", "CmdOrCtrl+E")?)
        .item(&item("share", "Share\u{2026}", "Shift+CmdOrCtrl+S")?)
        .separator()
        .close_window()
        .build()?;
    let edit = SubmenuBuilder::new(app, "Edit")
        .undo()
        .redo()
        .separator()
        .cut()
        .copy()
        .paste()
        .select_all()
        .build()?;
    let view = SubmenuBuilder::new(app, "View")
        .item(&item("view-library", "Library", "CmdOrCtrl+1")?)
        .item(&item("view-shorts", "Shorts", "CmdOrCtrl+2")?)
        .item(&item("view-shared", "Shared", "CmdOrCtrl+3")?)
        .separator()
        .item(&item(
            "toggle-sidebar",
            "Toggle Sidebar",
            "Ctrl+CmdOrCtrl+S",
        )?)
        .item(&item(
            "toggle-inspector",
            "Toggle Inspector",
            "Alt+CmdOrCtrl+I",
        )?)
        .separator()
        .fullscreen()
        .build()?;
    let window = SubmenuBuilder::new(app, "Window")
        .minimize()
        .maximize()
        .separator()
        .item(&item(
            "show-library",
            &format!("{} Library", BRAND.name),
            "CmdOrCtrl+0",
        )?)
        .build()?;
    let help = SubmenuBuilder::new(app, "Help")
        .text("help", format!("{} on the Web", BRAND.name))
        .build()?;
    MenuBuilder::new(app)
        .items(&[&app_menu, &file, &edit, &view, &window, &help])
        .build()
}

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

fn on_menu(app: &AppHandle, id: &str) {
    match id {
        "tray-start" => {
            let _ = app.emit("toggle-recording", ());
        }
        "tray-open" | "show-library" => show_main(app),
        "tray-quit" => app.exit(0),
        "help" => {
            let _ = std::process::Command::new("open").arg(&BRAND.repo).spawn();
        }
        other => {
            show_main(app);
            let _ = app.emit_to("main", "menu", other.to_string());
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let port = tauri::async_runtime::block_on(server::start()).expect("start local server");
    let toggle = Shortcut::new(Some(Modifiers::ALT | Modifiers::SHIFT), Code::KeyR);

    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .on_menu_event(|app, ev| on_menu(app, ev.id.as_ref()))
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
            tunnel_users: Default::default(),
        })
        .setup(move |app| {
            app.global_shortcut().register(toggle)?;
            // Phone remote buttons arrive from the pairing server as events for the controls window.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let mut rx = pairing::REMOTE.subscribe();
                while let Ok(action) = rx.recv().await {
                    let _ = handle.emit("remote", action);
                }
            });
            app.set_menu(menu_bar(app.handle())?)?;
            // Developer override for screenshots and testing: <SLUG>_APPEARANCE=light|dark.
            let theme = match std::env::var(format!(
                "{}_APPEARANCE",
                BRAND.slug.to_uppercase().replace('-', "_")
            ))
            .as_deref()
            {
                Ok("light") => Some(Theme::Light),
                Ok("dark") => Some(Theme::Dark),
                _ => None,
            };
            if theme.is_some() {
                for w in app.webview_windows().values() {
                    let _ = w.set_theme(theme);
                }
            }
            let start = MenuItem::with_id(
                app,
                "tray-start",
                "Start or stop recording (Alt+Shift+R)",
                true,
                None::<&str>,
            )?;
            let open = MenuItem::with_id(
                app,
                "tray-open",
                format!("Open {}", BRAND.name),
                true,
                None::<&str>,
            )?;
            let quit = MenuItem::with_id(
                app,
                "tray-quit",
                format!("Quit {}", BRAND.name),
                true,
                None::<&str>,
            )?;
            let menu = Menu::with_items(app, &[&start, &open, &quit])?;
            TrayIconBuilder::new()
                // Monochrome template image so macOS tints it like its own menu bar icons.
                .icon(tauri::image::Image::from_bytes(include_bytes!(
                    "../../assets/brand/tray@2x.png"
                ))?)
                .icon_as_template(true)
                .tooltip(&BRAND.name)
                .menu(&menu)
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
            commands::hud_layout,
            commands::phone_open,
            commands::phone_close,
            commands::phone_airdrop,
            commands::airplay_status,
            commands::open_airplay_settings,
            commands::android_wifi_pair,
            commands::android_wifi_connect,
            commands::phone_devices,
            commands::install_android_tools,
            commands::set_layout,
            commands::show_main,
            commands::bubble_size,
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
