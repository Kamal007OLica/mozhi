//! Menu-bar / system-tray icon: ஃ in the menu bar, with dictation, mode and settings.

use std::sync::Arc;

use parking_lot::Mutex;
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, Wry};

use crate::controller::Controller;
use crate::types::OutputMode;

const MODES: [(OutputMode, &str, &str); 4] = [
    (OutputMode::Auto, "mode:auto", "Auto · தானியங்கி"),
    (OutputMode::Tamil, "mode:tamil", "தமிழ் · Tamil script"),
    (OutputMode::Tanglish, "mode:tanglish", "Tanglish"),
    (OutputMode::English, "mode:english", "English"),
];

pub struct TrayModes(Mutex<Vec<(OutputMode, CheckMenuItem<Wry>)>>);

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let ctrl = app.state::<Arc<Controller>>().inner().clone();
    let current = ctrl.settings.get().output_mode;
    let shortcut = if cfg!(target_os = "macos") { "⌃⌃" } else { "Ctrl Ctrl" };

    let dictate = MenuItem::with_id(app, "dictate", format!("Start dictation    {shortcut}"), true, None::<&str>)?;
    let mut items = Vec::new();
    for (mode, id, label) in MODES {
        items.push((mode, CheckMenuItem::with_id(app, id, label, true, mode == current, None::<&str>)?));
    }
    let refs: Vec<&dyn tauri::menu::IsMenuItem<Wry>> = items.iter().map(|(_, i)| i as &dyn tauri::menu::IsMenuItem<Wry>).collect();
    let mode_menu = Submenu::with_items(app, "Write as", true, &refs)?;
    let open = MenuItem::with_id(app, "open", "Open Mozhi…", true, Some("CmdOrCtrl+,"))?;
    let quit = MenuItem::with_id(app, "quit", "Quit Mozhi", true, Some("CmdOrCtrl+Q"))?;
    let menu = Menu::with_items(
        app,
        &[&dictate, &mode_menu, &PredefinedMenuItem::separator(app)?, &open, &PredefinedMenuItem::separator(app)?, &quit],
    )?;
    app.manage(TrayModes(Mutex::new(items)));

    TrayIconBuilder::with_id("mozhi")
        .icon(Image::from_bytes(include_bytes!("../icons/tray-template.png"))?)
        .icon_as_template(true)
        .tooltip("Mozhi · மொழி")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(move |app, event| {
            let id = event.id().as_ref();
            match id {
                "dictate" => ctrl.toggle(),
                "open" => crate::show_main_window(app),
                "quit" => {
                    ctrl.cancel();
                    app.exit(0)
                }
                _ => {
                    if let Some((mode, _, _)) = MODES.iter().find(|(_, mid, _)| *mid == id) {
                        ctrl.set_mode(*mode);
                        sync_mode(app, *mode);
                    }
                }
            }
        })
        .build(app)?;
    Ok(())
}

/// Keep the check marks in step with the setting, whoever changed it.
pub fn sync_mode(app: &AppHandle, mode: OutputMode) {
    if let Some(state) = app.try_state::<TrayModes>() {
        for (m, item) in state.0.lock().iter() {
            let _ = item.set_checked(*m == mode);
        }
    }
}
