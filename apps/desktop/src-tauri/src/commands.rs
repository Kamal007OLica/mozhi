//! IPC commands. Mirrored by `src/shared/ipc.ts`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, State};
use tauri_plugin_autostart::ManagerExt;

use crate::controller::{Controller, EV_HISTORY, EV_SETTINGS};
use crate::engine::{self, Engine};
use crate::platform;
use crate::types::*;

pub const EV_DOWNLOAD: &str = "mozhi:download";

type Ctrl<'a> = State<'a, Arc<Controller>>;
type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[derive(Default)]
pub struct Downloads(Mutex<HashMap<String, Arc<AtomicBool>>>);

// ── settings ────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_settings(ctrl: Ctrl) -> Settings {
    ctrl.settings.get()
}

#[tauri::command]
pub fn set_settings(app: AppHandle, ctrl: Ctrl, settings: Settings) -> CmdResult<Settings> {
    let prev = ctrl.settings.get();
    ctrl.settings.set(settings.clone()).map_err(err)?;
    *ctrl.hotkey.write() = settings.hotkey.clone();
    ctrl.set_mode(settings.output_mode);

    if prev.launch_at_login != settings.launch_at_login {
        let auto = app.autolaunch();
        let r = if settings.launch_at_login { auto.enable() } else { auto.disable() };
        if let Err(e) = r {
            log::warn!("autostart: {e}");
        }
    }
    if prev.model_id != settings.model_id {
        crate::preload_model(&ctrl);
    }
    let _ = app.emit(EV_SETTINGS, &settings);
    Ok(settings)
}

// ── models ──────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_models(ctrl: Ctrl) -> Vec<ModelInfo> {
    engine::models::catalog(&ctrl.engine.models_dir())
}

#[tauri::command]
pub async fn download_model(app: AppHandle, id: String) -> CmdResult<()> {
    let ctrl = app.state::<Arc<Controller>>().inner().clone();
    let cancel = Arc::new(AtomicBool::new(false));
    app.state::<Downloads>().0.lock().insert(id.clone(), cancel.clone());

    let emitter = app.clone();
    let pid = id.clone();
    let last = Mutex::new(Instant::now() - Duration::from_secs(1));
    let progress = move |downloaded: u64, total: u64| {
        let mut l = last.lock();
        if l.elapsed() >= Duration::from_millis(100) || downloaded == total {
            *l = Instant::now();
            let _ = emitter.emit(EV_DOWNLOAD, DownloadProgress { id: pid.clone(), downloaded, total, done: false, error: None });
        }
    };
    let result = engine::models::download(&ctrl.engine.models_dir(), &id, cancel, progress).await;
    app.state::<Downloads>().0.lock().remove(&id);

    let error = result.as_ref().err().map(|e| format!("{e:#}"));
    let _ = app.emit(EV_DOWNLOAD, DownloadProgress { id: id.clone(), downloaded: 0, total: 0, done: error.is_none(), error: error.clone() });
    if error.is_none() && ctrl.model_id() == id {
        crate::preload_model(&ctrl);
    }
    result.map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub fn cancel_download(downloads: State<Downloads>, id: String) {
    if let Some(flag) = downloads.0.lock().get(&id) {
        flag.store(true, Ordering::SeqCst);
    }
}

#[tauri::command]
pub fn delete_model(ctrl: Ctrl, id: String) -> CmdResult<()> {
    engine::models::delete(&ctrl.engine.models_dir(), &id).map_err(err)
}

#[tauri::command]
pub fn input_devices() -> Vec<String> {
    Engine::input_devices()
}

// ── dictation ───────────────────────────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlaySnapshot {
    state: StatePayload,
    target: TargetPayload,
    settings: Settings,
}

#[tauri::command]
pub fn get_overlay_state(ctrl: Ctrl) -> OverlaySnapshot {
    let (state, target) = ctrl.snapshot();
    OverlaySnapshot { state, target, settings: ctrl.settings.get() }
}

#[tauri::command]
pub fn toggle_dictation(ctrl: Ctrl) {
    ctrl.inner().toggle();
}

#[tauri::command]
pub fn commit_dictation(ctrl: Ctrl) {
    ctrl.inner().commit();
}

#[tauri::command]
pub fn cancel_dictation(ctrl: Ctrl) {
    ctrl.inner().cancel();
}

#[tauri::command]
pub fn set_output_mode(ctrl: Ctrl, mode: OutputMode) {
    ctrl.set_mode(mode);
    crate::tray::sync_mode(ctrl.app_handle(), mode);
}

// ── overlay window ──────────────────────────────────────────────────────────

/// The overlay grows upward as the transcript gets longer: keep the bottom edge fixed.
#[tauri::command]
pub fn overlay_resize(app: AppHandle, width: f64, height: f64) -> CmdResult<()> {
    let w = app.get_webview_window("overlay").ok_or("no overlay")?;
    let scale = w.scale_factor().map_err(err)?;
    let pos = w.outer_position().map_err(err)?.to_logical::<f64>(scale);
    let size = w.outer_size().map_err(err)?.to_logical::<f64>(scale);
    let (width, height) = (width.clamp(240.0, 900.0), height.clamp(48.0, 640.0));
    if (size.width - width).abs() < 1.0 && (size.height - height).abs() < 1.0 {
        return Ok(());
    }
    let x = pos.x + (size.width - width) / 2.0;
    let y = pos.y + (size.height - height);
    w.set_size(LogicalSize::new(width, height)).map_err(err)?;
    w.set_position(LogicalPosition::new(x, y)).map_err(err)?;
    Ok(())
}

/// Called by the overlay after the user drags it; remembers the spot.
#[tauri::command]
pub fn overlay_dragged(app: AppHandle, ctrl: Ctrl) -> CmdResult<()> {
    let w = app.get_webview_window("overlay").ok_or("no overlay")?;
    let scale = w.scale_factor().map_err(err)?;
    let p = w.outer_position().map_err(err)?.to_logical::<f64>(scale);
    ctrl.settings.update(|s| s.overlay_position = Some((p.x, p.y))).map_err(err)?;
    Ok(())
}

// ── permissions ─────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_permissions() -> Permissions {
    platform::permissions()
}

#[tauri::command]
pub fn request_permission(kind: PermissionKind) {
    platform::request_permission(kind)
}

#[tauri::command]
pub fn open_permission_settings(kind: PermissionKind) {
    platform::open_permission_settings(kind)
}

// ── history ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_history(ctrl: Ctrl) -> Vec<HistoryItem> {
    ctrl.history.list()
}

#[tauri::command]
pub fn delete_history_item(app: AppHandle, ctrl: Ctrl, id: String) -> CmdResult<()> {
    ctrl.history.delete(&id).map_err(err)?;
    let _ = app.emit(EV_HISTORY, ());
    Ok(())
}

#[tauri::command]
pub fn clear_history(app: AppHandle, ctrl: Ctrl) -> CmdResult<()> {
    ctrl.history.clear().map_err(err)?;
    let _ = app.emit(EV_HISTORY, ());
    Ok(())
}

// ── misc ────────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn polish_status(config: PolishConfig) -> PolishStatus {
    engine::polish::status(&config).await
}

#[tauri::command]
pub fn copy_text(text: String) -> CmdResult<()> {
    platform::copy_to_clipboard(&text).map_err(err)
}

#[tauri::command]
pub fn open_main_window(app: AppHandle) {
    crate::show_main_window(&app);
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    version: String,
    os: String,
    arch: String,
    models_dir: String,
}

#[tauri::command]
pub fn app_info(app: AppHandle, ctrl: Ctrl) -> AppInfo {
    AppInfo {
        version: app.package_info().version.to_string(),
        os: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        models_dir: ctrl.engine.models_dir().display().to_string(),
    }
}
