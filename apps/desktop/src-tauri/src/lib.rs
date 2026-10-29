//! Mozhi · மொழி — on-device Tamil voice typing.
//!
//! Layout:
//!   types.rs       shared, serialisable types (mirrored in src/shared/ipc.ts)
//!   store.rs       settings + history JSON stores
//!   controller.rs  the dictation state machine
//!   commands.rs    IPC surface for the two webviews
//!   tray.rs        menu-bar icon
//!   engine/        audio → whisper.cpp → text (+ transliteration, LLM polish)
//!   platform/      hotkeys, focus tracking, paste, permissions, overlay window

mod commands;
mod controller;
mod engine;
mod platform;
mod store;
mod tray;
mod types;

use std::sync::Arc;
use std::thread;

use tauri::{Manager, WindowEvent};

use controller::Controller;
use engine::Engine;
use store::{History, SettingsStore};

pub fn show_main_window(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        platform::set_dock_visible(app, true);
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Load the selected model in the background so the first ⌃⌃ is instant.
pub(crate) fn preload_model(ctrl: &Arc<Controller>) {
    let engine = ctrl.engine.clone();
    let id = ctrl.model_id();
    let installed = engine::models::catalog(&engine.models_dir()).iter().any(|m| m.id == id && m.installed);
    if !installed {
        return;
    }
    thread::Builder::new()
        .name("mozhi-preload".into())
        .spawn(move || {
            let t = std::time::Instant::now();
            match engine.preload(&id) {
                Ok(()) => log::info!("model {id} ready in {:?}", t.elapsed()),
                Err(e) => log::warn!("preload {id}: {e:#}"),
            }
        })
        .ok();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| show_main_window(app)))
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .level_for("whisper_rs", log::LevelFilter::Warn)
                .build(),
        )
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .manage(commands::Downloads::default())
        .setup(|app| {
            let handle = app.handle().clone();
            let data_dir = app.path().app_data_dir()?;
            let config_dir = app.path().app_config_dir()?;
            std::fs::create_dir_all(data_dir.join("models"))?;

            let settings = Arc::new(SettingsStore::load(config_dir.join("settings.json")));
            let history = Arc::new(History::load(data_dir.join("history.json")));
            let engine = Engine::new(data_dir.join("models"));
            let ctrl = Controller::new(handle.clone(), engine, settings.clone(), history);
            app.manage(ctrl.clone());

            if let Some(overlay) = app.get_webview_window("overlay") {
                if let Err(e) = platform::setup_overlay(&overlay) {
                    log::warn!("overlay setup: {e:#}");
                }
            }

            let listener = ctrl.clone();
            platform::start_key_listener(ctrl.hotkey.clone(), ctrl.capture.clone(), Box::new(move |ev| listener.handle_key(ev)))?;

            tray::create(&handle)?;
            preload_model(&ctrl);

            // Silero VAD (0.9 MB) cuts hallucinated text from silence. New model downloads fetch
            // it too; this covers installs that already had a model before VAD shipped.
            let models_dir = ctrl.engine.models_dir();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = engine::models::ensure_vad(&models_dir).await {
                    log::warn!("VAD model not fetched (energy trimming will be used): {e:#}");
                }
            });

            if settings.get().onboarded {
                platform::set_dock_visible(&handle, false);
            } else {
                show_main_window(&handle);
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the settings window keeps Mozhi running in the menu bar.
            if window.label() == "main" {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                    platform::set_dock_visible(window.app_handle(), false);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::set_settings,
            commands::get_models,
            commands::download_model,
            commands::cancel_download,
            commands::delete_model,
            commands::input_devices,
            commands::get_overlay_state,
            commands::toggle_dictation,
            commands::commit_dictation,
            commands::cancel_dictation,
            commands::edit_dictation,
            commands::update_draft,
            commands::insert_draft,
            commands::open_history,
            commands::set_output_mode,
            commands::overlay_resize,
            commands::overlay_dragged,
            commands::get_permissions,
            commands::request_permission,
            commands::open_permission_settings,
            commands::get_history,
            commands::delete_history_item,
            commands::clear_history,
            commands::polish_status,
            commands::copy_text,
            commands::open_main_window,
            commands::app_info,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Mozhi");
}
