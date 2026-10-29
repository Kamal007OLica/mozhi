//! The dictation state machine.
//!
//!   Idle ──⌃⌃──▶ Starting ──▶ Listening ──⌃⌃ / ↵──▶ Transcribing ──▶ Inserting ──▶ Done ──▶ Idle
//!                   │             │                     │
//!                   └──── esc ────┴──────── esc ────────┴──▶ Idle (audio discarded)
//!
//! Every transition is emitted to the overlay as `mozhi:state`. A session counter makes
//! late results from a cancelled session harmless.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use parking_lot::{Mutex, RwLock};
use tauri::{AppHandle, Emitter, Manager};

use crate::engine::{self, Engine, EngineSink, RecordOptions};
use crate::platform::{self, InsertOptions, KeyEvent};
use crate::store::{now_ms, History, SettingsStore};
use crate::types::*;

pub const EV_STATE: &str = "mozhi:state";
pub const EV_LEVEL: &str = "mozhi:level";
pub const EV_PARTIAL: &str = "mozhi:partial";
pub const EV_TARGET: &str = "mozhi:target";
pub const EV_FINAL: &str = "mozhi:final";
pub const EV_SETTINGS: &str = "mozhi:settings";
pub const EV_HISTORY: &str = "mozhi:history";
pub const EV_NAVIGATE: &str = "mozhi:navigate";

const OVERLAY: &str = "overlay";
const TRACK_EVERY: Duration = Duration::from_millis(250);
const LINGER_DONE: Duration = Duration::from_millis(1400);
const LINGER_ERROR: Duration = Duration::from_millis(3200);

struct Inner {
    phase: Phase,
    target: Option<FrontApp>,
    element: Option<FocusedElement>,
    mode: OutputMode,
}

pub struct Controller {
    app: AppHandle,
    pub engine: Engine,
    pub settings: Arc<SettingsStore>,
    pub history: Arc<History>,
    pub hotkey: Arc<RwLock<HotkeyConfig>>,
    pub capture: Arc<AtomicBool>,
    session: AtomicU64,
    inner: Mutex<Inner>,
}

struct Sink {
    app: AppHandle,
    session: u64,
    ctrl: std::sync::Weak<Controller>,
}

impl Sink {
    fn live(&self) -> bool {
        self.ctrl.upgrade().map(|c| c.session.load(Ordering::SeqCst) == self.session).unwrap_or(false)
    }
}

impl EngineSink for Sink {
    fn level(&self, level: f32) {
        if self.live() {
            let _ = self.app.emit_to(OVERLAY, EV_LEVEL, level);
        }
    }
    fn partial(&self, text: &str) {
        if self.live() {
            let _ = self.app.emit_to(OVERLAY, EV_PARTIAL, text);
        }
    }
}

impl Controller {
    pub fn new(app: AppHandle, engine: Engine, settings: Arc<SettingsStore>, history: Arc<History>) -> Arc<Self> {
        let s = settings.get();
        Arc::new(Self {
            app,
            engine,
            hotkey: Arc::new(RwLock::new(s.hotkey.clone())),
            capture: Arc::new(AtomicBool::new(false)),
            settings,
            history,
            session: AtomicU64::new(0),
            inner: Mutex::new(Inner { phase: Phase::Idle, target: None, element: None, mode: s.output_mode }),
        })
    }

    pub fn app_handle(&self) -> &AppHandle {
        &self.app
    }

    pub fn phase(&self) -> Phase {
        self.inner.lock().phase
    }

    pub fn snapshot(&self) -> (StatePayload, TargetPayload) {
        let i = self.inner.lock();
        (
            StatePayload { phase: i.phase, message: None, mode: i.mode },
            TargetPayload { app: i.target.clone(), element: i.element.clone() },
        )
    }

    pub fn model_id(&self) -> String {
        let id = self.settings.get().model_id;
        if id.is_empty() {
            engine::models::default_model_id().to_string()
        } else {
            id
        }
    }

    fn set_phase(&self, phase: Phase, message: Option<String>) {
        let mode = {
            let mut i = self.inner.lock();
            i.phase = phase;
            i.mode
        };
        let _ = self.app.emit(EV_STATE, StatePayload { phase, message, mode });
    }

    fn overlay(&self) -> Option<tauri::WebviewWindow> {
        self.app.get_webview_window(OVERLAY)
    }

    // ── input ──────────────────────────────────────────────────────────────

    pub fn handle_key(self: &Arc<Self>, ev: KeyEvent) {
        let phase = self.phase();
        match (ev, phase) {
            (KeyEvent::DoubleTap, Phase::Idle | Phase::Done | Phase::Error) => self.start(),
            (KeyEvent::DoubleTap, Phase::Starting | Phase::Listening) => self.commit(),
            (KeyEvent::Enter, Phase::Starting | Phase::Listening) if self.settings.get().commit_on_enter => self.commit(),
            (KeyEvent::Escape, Phase::Starting | Phase::Listening | Phase::Transcribing) => self.cancel(),
            (KeyEvent::Escape, Phase::Done | Phase::Error) => self.dismiss(),
            _ => {}
        }
    }

    pub fn toggle(self: &Arc<Self>) {
        self.handle_key(KeyEvent::DoubleTap);
    }

    // ── transitions ────────────────────────────────────────────────────────

    pub fn start(self: &Arc<Self>) {
        let settings = self.settings.get();
        let model_id = self.model_id();
        let session = self.session.fetch_add(1, Ordering::SeqCst) + 1;

        let installed = engine::models::catalog(&self.engine.models_dir()).iter().any(|m| m.id == model_id && m.installed);
        if !installed {
            self.show_overlay(&settings);
            self.set_phase(Phase::Error, Some("No speech model yet — download one in Mozhi → Models".into()));
            let _ = self.app.emit(EV_NAVIGATE, "models");
            crate::show_main_window(&self.app);
            self.hide_later(session, LINGER_ERROR);
            return;
        }

        let target = self.current_target();
        let element = platform::focused_element();
        {
            let mut i = self.inner.lock();
            i.target = target.clone();
            i.element = element.clone();
            i.mode = settings.output_mode;
        }
        let _ = self.app.emit(EV_TARGET, TargetPayload { app: target, element });
        let _ = self.app.emit_to(OVERLAY, EV_PARTIAL, "");
        self.set_phase(Phase::Starting, None);
        self.show_overlay(&settings);
        self.capture.store(true, Ordering::SeqCst);

        let sink = Arc::new(Sink { app: self.app.clone(), session, ctrl: Arc::downgrade(self) });
        let opts = RecordOptions {
            device: settings.mic_device.clone(),
            model_id,
            mode: settings.output_mode,
            live_preview: settings.live_preview,
        };
        match self.engine.start(opts, sink) {
            Ok(()) => {
                self.set_phase(Phase::Listening, None);
                self.spawn_tracker(session);
            }
            Err(e) => {
                log::error!("could not start recording: {e:#}");
                self.capture.store(false, Ordering::SeqCst);
                self.set_phase(Phase::Error, Some(friendly_error(&e)));
                self.hide_later(session, LINGER_ERROR);
            }
        }
    }

    pub fn commit(self: &Arc<Self>) {
        if !matches!(self.phase(), Phase::Starting | Phase::Listening) {
            return;
        }
        let session = self.session.load(Ordering::SeqCst);
        self.set_phase(Phase::Transcribing, None);
        let me = self.clone();
        thread::Builder::new()
            .name("mozhi-commit".into())
            .spawn(move || me.finish_session(session))
            .ok();
    }

    fn finish_session(self: Arc<Self>, session: u64) {
        let live = || self.session.load(Ordering::SeqCst) == session;
        let settings = self.settings.get();
        let mode = settings.output_mode;

        let result = match self.engine.finish() {
            Ok(r) => r,
            Err(e) => {
                if live() {
                    log::error!("transcription failed: {e:#}");
                    self.capture.store(false, Ordering::SeqCst);
                    self.set_phase(Phase::Error, Some(friendly_error(&e)));
                    self.hide_later(session, LINGER_ERROR);
                }
                return;
            }
        };
        if !live() {
            return;
        }

        let mut text = result.text.trim().to_string();
        if text.is_empty() {
            self.capture.store(false, Ordering::SeqCst);
            self.set_phase(Phase::Done, Some("Didn't catch that — try again".into()));
            self.hide_later(session, LINGER_DONE);
            return;
        }

        if settings.polish.enabled {
            let cfg = settings.polish.clone();
            let input = text.clone();
            let polished = tauri::async_runtime::block_on(async move {
                tokio::time::timeout(Duration::from_millis(cfg.timeout_ms as u64), engine::polish::polish(&cfg, &input, mode)).await
            });
            match polished {
                Ok(Ok(p)) if !p.trim().is_empty() => text = p.trim().to_string(),
                Ok(Err(e)) => log::warn!("polish failed, using raw transcript: {e:#}"),
                Err(_) => log::warn!("polish timed out, using raw transcript"),
                _ => {}
            }
            if !live() {
                return;
            }
        }

        let _ = self.app.emit(EV_FINAL, &text);
        self.capture.store(false, Ordering::SeqCst);

        // Paste into the app the user is in *now* — they may have switched while speaking.
        let target = self.current_target().or_else(|| self.inner.lock().target.clone());
        let pasteable = settings.auto_paste && target.as_ref().map(|t| !t.is_self).unwrap_or(false);

        let (inserted, message) = if pasteable {
            let target = target.clone().unwrap();
            self.set_phase(Phase::Inserting, None);
            match platform::insert_text(&text, &target, InsertOptions { restore_clipboard: settings.restore_clipboard }) {
                Ok(()) => (true, format!("Inserted into {}", target.name)),
                Err(e) => {
                    log::error!("paste failed: {e:#}");
                    let _ = platform::copy_to_clipboard(&text);
                    (false, "Couldn't paste — copied to clipboard instead".to_string())
                }
            }
        } else {
            match platform::copy_to_clipboard(&text) {
                Ok(()) => (false, "Copied to clipboard".to_string()),
                Err(_) => (false, "Ready — copy it from Mozhi → History".to_string()),
            }
        };

        if settings.history_enabled {
            self.history.add(HistoryItem {
                id: format!("{}-{}", now_ms(), session),
                at: now_ms(),
                text: text.clone(),
                raw: result.raw.clone(),
                language: result.language.clone(),
                mode,
                app_name: target.map(|t| t.name),
                audio_ms: result.audio_ms,
                inference_ms: result.inference_ms,
                inserted,
            });
            let _ = self.app.emit(EV_HISTORY, ());
        }

        self.set_phase(Phase::Done, Some(message));
        self.hide_later(session, LINGER_DONE);
    }

    pub fn cancel(self: &Arc<Self>) {
        self.session.fetch_add(1, Ordering::SeqCst);
        self.engine.cancel();
        self.capture.store(false, Ordering::SeqCst);
        self.set_phase(Phase::Idle, None);
        self.hide_overlay();
    }

    fn dismiss(self: &Arc<Self>) {
        self.session.fetch_add(1, Ordering::SeqCst);
        self.capture.store(false, Ordering::SeqCst);
        self.set_phase(Phase::Idle, None);
        self.hide_overlay();
    }

    pub fn set_mode(&self, mode: OutputMode) {
        if let Ok(s) = self.settings.update(|s| s.output_mode = mode) {
            self.inner.lock().mode = mode;
            let _ = self.app.emit(EV_SETTINGS, &s);
        }
    }

    // ── helpers ────────────────────────────────────────────────────────────

    fn current_target(&self) -> Option<FrontApp> {
        platform::frontmost_app().map(|mut app| {
            if app.icon.is_none() {
                app.icon = platform::app_icon(&app);
            }
            app
        })
    }

    /// While listening, follow the user across apps so the overlay always names the real target.
    fn spawn_tracker(self: &Arc<Self>, session: u64) {
        let me = Arc::downgrade(self);
        thread::Builder::new()
            .name("mozhi-target".into())
            .spawn(move || loop {
                thread::sleep(TRACK_EVERY);
                let Some(me) = me.upgrade() else { return };
                if me.session.load(Ordering::SeqCst) != session || !matches!(me.phase(), Phase::Starting | Phase::Listening) {
                    return;
                }
                let app = platform::frontmost_app();
                let element = platform::focused_element();
                let changed = {
                    let i = me.inner.lock();
                    let app_changed = match (&i.target, &app) {
                        (Some(a), Some(b)) => a.pid != b.pid || a.window_title != b.window_title,
                        (None, None) => false,
                        _ => true,
                    };
                    app_changed || i.element != element
                };
                if changed {
                    let app = app.map(|mut a| {
                        a.icon = platform::app_icon(&a);
                        a
                    });
                    {
                        let mut i = me.inner.lock();
                        i.target = app.clone();
                        i.element = element.clone();
                    }
                    let _ = me.app.emit(EV_TARGET, TargetPayload { app, element });
                }
            })
            .ok();
    }

    fn show_overlay(&self, settings: &Settings) {
        if let Some(w) = self.overlay() {
            if let Err(e) = platform::show_overlay(&w, settings.overlay_position) {
                log::warn!("show overlay: {e:#}");
            }
        }
    }

    fn hide_overlay(&self) {
        if let Some(w) = self.overlay() {
            let _ = platform::hide_overlay(&w);
        }
    }

    fn hide_later(self: &Arc<Self>, session: u64, after: Duration) {
        let me = Arc::downgrade(self);
        thread::spawn(move || {
            thread::sleep(after);
            if let Some(me) = me.upgrade() {
                if me.session.load(Ordering::SeqCst) == session && matches!(me.phase(), Phase::Done | Phase::Error) {
                    me.set_phase(Phase::Idle, None);
                    me.hide_overlay();
                }
            }
        });
    }
}

fn friendly_error(e: &anyhow::Error) -> String {
    let s = format!("{e:#}").to_lowercase();
    if s.contains("permission") || s.contains("not authorized") || s.contains("denied") {
        "Microphone is blocked — allow Mozhi in Settings".into()
    } else if s.contains("device") || s.contains("no input") {
        "No microphone found".into()
    } else if s.contains("model") {
        "Speech model failed to load — re-download it in Models".into()
    } else {
        format!("Something went wrong: {e}")
    }
}
