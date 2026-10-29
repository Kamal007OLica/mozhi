//! OS integration — the global double-tap listener, frontmost-app / focused-field
//! tracking, text insertion, permissions and the non-activating overlay window.
//!
//! CONTRACT: the controller (src/controller.rs) depends only on the public items in
//! this file. Keep these signatures stable; put per-OS code in private submodules
//! (`macos.rs`, `windows.rs`) behind `#[cfg]`, with a no-op fallback for other targets.

mod detector;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as imp;

#[cfg(windows)]
#[path = "windows.rs"]
mod win;
#[cfg(windows)]
use win as imp;

#[cfg(not(any(target_os = "macos", windows)))]
mod fallback;
#[cfg(not(any(target_os = "macos", windows)))]
use fallback as imp;

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use parking_lot::RwLock;

use crate::types::{FocusedElement, FrontApp, HotkeyConfig, PermissionKind, Permissions};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyEvent {
    /// The trigger key was tapped twice (see `HotkeyConfig`). Never consumed.
    DoubleTap,
    /// Return / keypad Enter, only while `capture` is true. Consumed (the focused app never sees it).
    Enter,
    /// Escape, only while `capture` is true. Consumed.
    Escape,
}

/// Starts the global keyboard hook on its own thread. Call once at startup.
///
/// * `hotkey` is re-read on every event, so settings changes apply live.
/// * While `capture` is false, Enter/Escape pass through untouched and are not reported.
/// * Must not add perceptible latency to normal typing; never panics on unknown keys.
/// * macOS: active CGEventTap (needs Accessibility). If permission is missing, keep
///   retrying quietly every ~2 s until granted instead of failing.
/// * Windows: WH_KEYBOARD_LL hook with its own message loop.
pub fn start_key_listener(
    hotkey: Arc<RwLock<HotkeyConfig>>,
    capture: Arc<AtomicBool>,
    on_event: Box<dyn Fn(KeyEvent) + Send + Sync>,
) -> anyhow::Result<()> {
    imp::start_key_listener(hotkey, capture, on_event)
}

/// The app that currently has keyboard focus (what the user is typing into).
/// `icon` may be None here; use [`app_icon`] (cached) to fill it.
pub fn frontmost_app() -> Option<FrontApp> {
    imp::frontmost_app()
}

/// PNG icon for the app as a `data:image/png;base64,…` URL, ~64 px. Cached per app id.
pub fn app_icon(app: &FrontApp) -> Option<String> {
    imp::app_icon(app)
}

/// The focused UI element of the frontmost app, via Accessibility (macOS) / UI Automation (Windows).
/// Best effort — return None rather than block for more than ~30 ms.
pub fn focused_element() -> Option<FocusedElement> {
    imp::focused_element()
}

#[derive(Debug, Clone, Copy)]
pub struct InsertOptions {
    /// Put the previous clipboard contents back ~300 ms after pasting.
    pub restore_clipboard: bool,
}

/// Inserts `text` into `target`: re-activates it if it is no longer frontmost, writes the
/// clipboard, sends ⌘V / Ctrl+V, optionally restores the clipboard. Blocking (<500 ms).
pub fn insert_text(text: &str, target: &FrontApp, opts: InsertOptions) -> anyhow::Result<()> {
    imp::insert_text(text, target, opts)
}

pub fn copy_to_clipboard(text: &str) -> anyhow::Result<()> {
    imp::copy_to_clipboard(text)
}

pub fn permissions() -> Permissions {
    imp::permissions()
}

/// Triggers the system prompt where one exists (Accessibility trust prompt, mic request).
pub fn request_permission(kind: PermissionKind) {
    imp::request_permission(kind)
}

/// Opens the relevant System Settings / Windows Settings pane.
pub fn open_permission_settings(kind: PermissionKind) {
    imp::open_permission_settings(kind)
}

/// One-time setup of the overlay window: floats above full-screen apps on every Space,
/// never becomes key/main (so the user's app keeps focus and the caret), accepts clicks.
/// macOS: convert to a non-activating NSPanel. Windows: WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW.
pub fn setup_overlay(window: &tauri::WebviewWindow) -> anyhow::Result<()> {
    imp::setup_overlay(window)
}

/// Shows the overlay without activating Mozhi. `position` is a saved top-left in logical px;
/// None = horizontally centred, 72 px above the bottom of the work area of the screen
/// containing the mouse cursor.
pub fn show_overlay(window: &tauri::WebviewWindow, position: Option<(f64, f64)>) -> anyhow::Result<()> {
    imp::show_overlay(window, position)
}

pub fn hide_overlay(window: &tauri::WebviewWindow) -> anyhow::Result<()> {
    imp::hide_overlay(window)
}

/// Lets the overlay take keyboard focus so the transcript can be edited. On macOS Mozhi is
/// still not activated: the panel becomes key while the user's app stays frontmost.
pub fn focus_overlay(window: &tauri::WebviewWindow) -> anyhow::Result<()> {
    imp::focus_overlay(window)
}

/// Gives keyboard focus back to `target` (blocking, < 150 ms) so a paste lands there.
pub fn unfocus_overlay(window: &tauri::WebviewWindow, target: Option<&FrontApp>) -> anyhow::Result<()> {
    imp::unfocus_overlay(window, target)
}

/// macOS: menu-bar app (no Dock icon) until the settings window opens. No-op elsewhere.
pub fn set_dock_visible(app: &tauri::AppHandle, visible: bool) {
    imp::set_dock_visible(app, visible)
}
