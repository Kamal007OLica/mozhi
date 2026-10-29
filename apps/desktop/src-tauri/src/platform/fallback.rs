//! Other targets (Linux, BSD): no global hook or focus tracking yet. Clipboard-only insertion.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use parking_lot::RwLock;

use super::{InsertOptions, KeyEvent};
use crate::types::{FocusedElement, FrontApp, HotkeyConfig, PermState, PermissionKind, Permissions};

pub fn start_key_listener(
    _hotkey: Arc<RwLock<HotkeyConfig>>,
    _capture: Arc<AtomicBool>,
    _on_event: Box<dyn Fn(KeyEvent) + Send + Sync>,
) -> anyhow::Result<()> {
    log::warn!("global double-tap is not supported on this OS yet; use the tray menu");
    Ok(())
}

pub fn frontmost_app() -> Option<FrontApp> {
    None
}

pub fn app_icon(_app: &FrontApp) -> Option<String> {
    None
}

pub fn focused_element() -> Option<FocusedElement> {
    None
}

pub fn insert_text(text: &str, _target: &FrontApp, _opts: InsertOptions) -> anyhow::Result<()> {
    copy_to_clipboard(text)?;
    anyhow::bail!("automatic paste is not supported on this OS yet, so the text was copied")
}

pub fn copy_to_clipboard(text: &str) -> anyhow::Result<()> {
    arboard::Clipboard::new()?.set_text(text.to_string())?;
    Ok(())
}

pub fn permissions() -> Permissions {
    Permissions { microphone: PermState::NotRequired, accessibility: PermState::NotRequired }
}

pub fn request_permission(_kind: PermissionKind) {}

pub fn open_permission_settings(_kind: PermissionKind) {}

pub fn setup_overlay(_window: &tauri::WebviewWindow) -> anyhow::Result<()> {
    Ok(())
}

pub fn show_overlay(window: &tauri::WebviewWindow, position: Option<(f64, f64)>) -> anyhow::Result<()> {
    if let Some((x, y)) = position {
        window.set_position(tauri::LogicalPosition::new(x, y))?;
    }
    window.show()?;
    Ok(())
}

pub fn hide_overlay(window: &tauri::WebviewWindow) -> anyhow::Result<()> {
    window.hide()?;
    Ok(())
}

pub fn focus_overlay(window: &tauri::WebviewWindow) -> anyhow::Result<()> {
    window.set_focus()?;
    Ok(())
}

pub fn unfocus_overlay(_window: &tauri::WebviewWindow, _target: Option<&FrontApp>) -> anyhow::Result<()> {
    Ok(())
}

pub fn set_dock_visible(_app: &tauri::AppHandle, _visible: bool) {}
