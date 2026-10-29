//! Types shared by the controller, the engine, the platform layer and the UI.
//! Everything here crosses the IPC boundary, so it is serialised in camelCase and
//! mirrored by hand in `src/shared/ipc.ts`. Change both together.

use serde::{Deserialize, Serialize};

/// What the user wants written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum OutputMode {
    /// Detect per utterance: Tamil speech → Tamil script, English speech → English.
    #[default]
    Auto,
    /// Tamil script (தமிழ்). English words said mid-sentence stay in English when polish is on.
    Tamil,
    /// Tamil speech written in Latin letters ("naan veetukku poren").
    Tanglish,
    /// English.
    English,
}

/// Which key is double-tapped to start/stop dictation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum TriggerKey {
    /// Either Control key (Ctrl on Windows).
    #[default]
    Control,
    RightControl,
    /// Either Option (macOS) / Alt (Windows).
    Option,
    RightOption,
    /// Fn / Globe — macOS only.
    Fn,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct HotkeyConfig {
    pub key: TriggerKey,
    /// Max gap between the two taps.
    pub double_tap_ms: u32,
    /// A tap is a press+release shorter than this, with no other key in between.
    pub max_tap_ms: u32,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self { key: TriggerKey::Control, double_tap_ms: 380, max_tap_ms: 260 }
    }
}

/// Optional clean-up by a local LLM served by Ollama (or any OpenAI-compatible endpoint).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct PolishConfig {
    pub enabled: bool,
    /// Base URL, e.g. http://127.0.0.1:11434
    pub endpoint: String,
    pub model: String,
    /// Give up and paste the raw transcript after this long.
    pub timeout_ms: u32,
}

impl Default for PolishConfig {
    fn default() -> Self {
        Self { enabled: false, endpoint: "http://127.0.0.1:11434".into(), model: "gemma3:4b".into(), timeout_ms: 6000 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub output_mode: OutputMode,
    pub model_id: String,
    pub hotkey: HotkeyConfig,
    /// Enter (while the overlay is listening) inserts, like a second double-tap.
    pub commit_on_enter: bool,
    /// Stop in the overlay with the transcript editable instead of pasting right away.
    pub review_before_insert: bool,
    /// Paste into the target app. When false Mozhi only copies to the clipboard.
    pub auto_paste: bool,
    /// Put the user's previous clipboard back after pasting.
    pub restore_clipboard: bool,
    /// Show a rolling transcript while speaking (uses extra CPU/GPU).
    pub live_preview: bool,
    pub polish: PolishConfig,
    /// Input device name; None = system default.
    pub mic_device: Option<String>,
    pub sounds: bool,
    pub launch_at_login: bool,
    /// Interface language: "ta" or "en".
    pub ui_language: String,
    pub history_enabled: bool,
    /// Where the user dragged the overlay to (logical px, top-left). None = bottom-centre.
    pub overlay_position: Option<(f64, f64)>,
    pub onboarded: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            output_mode: OutputMode::Auto,
            model_id: String::new(), // empty → engine::models::default_model_id()
            hotkey: HotkeyConfig::default(),
            commit_on_enter: true,
            review_before_insert: false,
            auto_paste: true,
            restore_clipboard: true,
            live_preview: true,
            polish: PolishConfig::default(),
            mic_device: None,
            sounds: true,
            launch_at_login: false,
            ui_language: "ta".into(),
            history_enabled: true,
            overlay_position: None,
            onboarded: false,
        }
    }
}

/// The app the user is typing into. Shown in the overlay — it must always be visible
/// so text never lands in the wrong place.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontApp {
    pub name: String,
    /// macOS bundle id, or the executable name on Windows (e.g. "slack.exe").
    pub id: String,
    pub pid: i32,
    pub window_title: Option<String>,
    /// PNG app icon as a data: URL, when available.
    pub icon: Option<String>,
    /// True when the frontmost app is Mozhi itself (nothing to paste into).
    pub is_self: bool,
}

/// The focused control inside the target app — "where" the text goes. Best effort.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FocusedElement {
    /// Normalised role: "textField", "textArea", "searchField", "comboBox", "webArea", "other".
    pub role: String,
    /// Accessible label / placeholder / title, e.g. "Message #general".
    pub label: Option<String>,
    /// Whether the element looks like it accepts typed text.
    pub editable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptResult {
    /// Final text, already converted for the requested OutputMode (before LLM polish).
    pub text: String,
    /// Whisper output before any conversion.
    pub raw: String,
    /// Detected or forced language code, e.g. "ta", "en".
    pub language: String,
    pub audio_ms: u64,
    pub inference_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    /// One line for the model picker.
    pub description: String,
    pub size_mb: u32,
    /// Rough relative speed on an M1 (1 = slowest in catalog, higher = faster).
    pub speed: u8,
    /// Rough Tamil accuracy (1–5).
    pub tamil_quality: u8,
    pub url: String,
    pub sha256: Option<String>,
    pub recommended: bool,
    pub installed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PolishStatus {
    pub reachable: bool,
    /// Models the endpoint reports as installed.
    pub models: Vec<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PermState {
    Granted,
    Denied,
    NotDetermined,
    /// Not applicable on this OS.
    NotRequired,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Permissions {
    pub microphone: PermState,
    /// macOS: needed to read the focused field, intercept Enter/Esc and paste.
    pub accessibility: PermState,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PermissionKind {
    Microphone,
    Accessibility,
}

/// Overlay lifecycle, emitted as `mozhi:state`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    Idle,
    /// Loading the model / opening the mic.
    Starting,
    Listening,
    /// Final transcription + polish.
    Transcribing,
    /// The final transcript is in the overlay for the user to edit; ↵ inserts it.
    Editing,
    Inserting,
    /// Inserted (or copied) — overlay lingers briefly then hides.
    Done,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StatePayload {
    pub phase: Phase,
    /// Human-readable detail for Done/Error ("Pasted into Slack", "Microphone blocked").
    pub message: Option<String>,
    pub mode: OutputMode,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TargetPayload {
    pub app: Option<FrontApp>,
    pub element: Option<FocusedElement>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryItem {
    pub id: String,
    /// Unix ms.
    pub at: u64,
    pub text: String,
    pub raw: String,
    pub language: String,
    pub mode: OutputMode,
    pub app_name: Option<String>,
    pub audio_ms: u64,
    pub inference_ms: u64,
    pub inserted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub id: String,
    pub downloaded: u64,
    pub total: u64,
    pub done: bool,
    pub error: Option<String>,
}
