// Typed bridge to the Rust core. Mirrors src-tauri/src/types.rs and commands.rs —
// change them together.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type OutputMode = "auto" | "tamil" | "tanglish" | "english";
export type TriggerKey = "control" | "rightControl" | "option" | "rightOption" | "fn";
export type Phase = "idle" | "starting" | "listening" | "transcribing" | "editing" | "inserting" | "done" | "error";
export type PermState = "granted" | "denied" | "notDetermined" | "notRequired";
export type PermissionKind = "microphone" | "accessibility";

export interface HotkeyConfig {
  key: TriggerKey;
  doubleTapMs: number;
  maxTapMs: number;
}

export interface PolishConfig {
  enabled: boolean;
  endpoint: string;
  model: string;
  timeoutMs: number;
}

export interface Settings {
  outputMode: OutputMode;
  modelId: string;
  hotkey: HotkeyConfig;
  commitOnEnter: boolean;
  reviewBeforeInsert: boolean;
  autoPaste: boolean;
  restoreClipboard: boolean;
  livePreview: boolean;
  polish: PolishConfig;
  micDevice: string | null;
  sounds: boolean;
  launchAtLogin: boolean;
  uiLanguage: "ta" | "en";
  historyEnabled: boolean;
  overlayPosition: [number, number] | null;
  onboarded: boolean;
}

export interface FrontApp {
  name: string;
  id: string;
  pid: number;
  windowTitle: string | null;
  icon: string | null;
  isSelf: boolean;
}

export interface FocusedElement {
  role: string;
  label: string | null;
  editable: boolean;
}

export interface ModelInfo {
  id: string;
  name: string;
  description: string;
  sizeMb: number;
  speed: number;
  tamilQuality: number;
  url: string;
  sha256: string | null;
  recommended: boolean;
  installed: boolean;
}

export interface PolishStatus {
  reachable: boolean;
  models: string[];
  error: string | null;
}

export interface Permissions {
  microphone: PermState;
  accessibility: PermState;
}

export interface StatePayload {
  phase: Phase;
  message: string | null;
  mode: OutputMode;
}

export interface TargetPayload {
  app: FrontApp | null;
  element: FocusedElement | null;
}

export interface HistoryItem {
  id: string;
  at: number;
  text: string;
  raw: string;
  language: string;
  mode: OutputMode;
  appName: string | null;
  audioMs: number;
  inferenceMs: number;
  inserted: boolean;
}

export interface DownloadProgress {
  id: string;
  downloaded: number;
  total: number;
  done: boolean;
  error: string | null;
}

export interface OverlaySnapshot {
  state: StatePayload;
  target: TargetPayload;
  settings: Settings;
}

export interface AppInfo {
  version: string;
  os: string;
  arch: string;
  modelsDir: string;
}

/** True when running inside Tauri (false in a plain browser, e.g. `vite` preview). */
export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export const api = {
  getSettings: () => invoke<Settings>("get_settings"),
  setSettings: (settings: Settings) => invoke<Settings>("set_settings", { settings }),
  getModels: () => invoke<ModelInfo[]>("get_models"),
  downloadModel: (id: string) => invoke<void>("download_model", { id }),
  cancelDownload: (id: string) => invoke<void>("cancel_download", { id }),
  deleteModel: (id: string) => invoke<void>("delete_model", { id }),
  inputDevices: () => invoke<string[]>("input_devices"),
  getOverlayState: () => invoke<OverlaySnapshot>("get_overlay_state"),
  toggleDictation: () => invoke<void>("toggle_dictation"),
  commitDictation: () => invoke<void>("commit_dictation"),
  cancelDictation: () => invoke<void>("cancel_dictation"),
  editDictation: () => invoke<void>("edit_dictation"),
  updateDraft: (text: string) => invoke<void>("update_draft", { text }),
  insertDraft: (text: string) => invoke<void>("insert_draft", { text }),
  openHistory: () => invoke<void>("open_history"),
  setOutputMode: (mode: OutputMode) => invoke<void>("set_output_mode", { mode }),
  overlayResize: (width: number, height: number) => invoke<void>("overlay_resize", { width, height }),
  overlayDragged: () => invoke<void>("overlay_dragged"),
  getPermissions: () => invoke<Permissions>("get_permissions"),
  requestPermission: (kind: PermissionKind) => invoke<void>("request_permission", { kind }),
  openPermissionSettings: (kind: PermissionKind) => invoke<void>("open_permission_settings", { kind }),
  getHistory: () => invoke<HistoryItem[]>("get_history"),
  deleteHistoryItem: (id: string) => invoke<void>("delete_history_item", { id }),
  clearHistory: () => invoke<void>("clear_history"),
  polishStatus: (config: PolishConfig) => invoke<PolishStatus>("polish_status", { config }),
  copyText: (text: string) => invoke<void>("copy_text", { text }),
  openMainWindow: () => invoke<void>("open_main_window"),
  appInfo: () => invoke<AppInfo>("app_info"),
};

export const events = {
  onState: (cb: (p: StatePayload) => void) => on("mozhi:state", cb),
  onLevel: (cb: (level: number) => void) => on("mozhi:level", cb),
  onPartial: (cb: (text: string) => void) => on("mozhi:partial", cb),
  onTarget: (cb: (p: TargetPayload) => void) => on("mozhi:target", cb),
  onFinal: (cb: (text: string) => void) => on("mozhi:final", cb),
  onSettings: (cb: (s: Settings) => void) => on("mozhi:settings", cb),
  onHistory: (cb: () => void) => on("mozhi:history", cb),
  onNavigate: (cb: (page: string) => void) => on("mozhi:navigate", cb),
  onDownload: (cb: (p: DownloadProgress) => void) => on("mozhi:download", cb),
};

function on<T>(name: string, cb: (payload: T) => void): Promise<UnlistenFn> {
  if (!inTauri) return Promise.resolve(() => {});
  return listen<T>(name, (e) => cb(e.payload));
}

export const MODE_LABELS: Record<OutputMode, { ta: string; en: string; short: string }> = {
  auto: { ta: "தானியங்கி", en: "Auto", short: "Auto" },
  tamil: { ta: "தமிழ்", en: "Tamil script", short: "தமிழ்" },
  tanglish: { ta: "தங்கிலிஷ்", en: "Tanglish", short: "Tanglish" },
  english: { ta: "ஆங்கிலம்", en: "English", short: "English" },
};

export const isMac = typeof navigator !== "undefined" && /Mac/i.test(navigator.platform || navigator.userAgent);
export const triggerGlyph = (key: TriggerKey) =>
  ({
    control: isMac ? "⌃" : "Ctrl",
    rightControl: isMac ? "right ⌃" : "Right Ctrl",
    option: isMac ? "⌥" : "Alt",
    rightOption: isMac ? "right ⌥" : "Right Alt",
    fn: "fn",
  })[key];
