// In-browser stand-in for the Rust core, so `vite` alone can preview the UI.
// Never used inside Tauri.
import type { api as Api } from "../shared/ipc";
import type { HistoryItem, ModelInfo, Settings } from "../shared/ipc";

let settings: Settings = {
  outputMode: "auto",
  modelId: "",
  hotkey: { key: "control", doubleTapMs: 380, maxTapMs: 260 },
  commitOnEnter: true,
  reviewBeforeInsert: false,
  autoPaste: true,
  restoreClipboard: true,
  livePreview: true,
  polish: { enabled: false, endpoint: "http://127.0.0.1:11434", model: "gemma3:4b", timeoutMs: 6000 },
  micDevice: null,
  sounds: true,
  launchAtLogin: false,
  uiLanguage: "ta",
  historyEnabled: true,
  overlayPosition: null,
  onboarded: new URLSearchParams(location.search).has("onboarded"),
};

const models: ModelInfo[] = [
  { id: "tamil-small-q8", name: "Tamil Small · fine-tuned", description: "Whisper fine-tuned on Tamil by IIT Madras SPRING Lab. The most accurate for Tamil, small and fast.", sizeMb: 264, speed: 4, tamilQuality: 4, url: "", sha256: null, recommended: true, installed: true },
  { id: "large-v3-turbo-q5", name: "Whisper Large v3 Turbo · Q5", description: "Choose this if you mix in a lot of English: Tamil, Tanglish and English in one model.", sizeMb: 574, speed: 4, tamilQuality: 3, url: "", sha256: null, recommended: false, installed: false },
  { id: "large-v3-q5", name: "Whisper Large v3 · Q5", description: "Most accurate multilingual model; slower.", sizeMb: 1080, speed: 2, tamilQuality: 5, url: "", sha256: null, recommended: false, installed: false },
  { id: "small-q5", name: "Whisper Small · Q5", description: "Light and quick. Fine for English, rough on Tamil.", sizeMb: 190, speed: 5, tamilQuality: 2, url: "", sha256: null, recommended: false, installed: false },
];

const history: HistoryItem[] = [
  { id: "1", at: Date.now() - 600000, text: "வணக்கம் எல்லாருக்கும், இன்னைக்கு meeting-க்கு கொஞ்சம் late ஆகும்.", raw: "", language: "ta", mode: "tamil", appName: "Slack", audioMs: 5200, inferenceMs: 640, inserted: true },
  { id: "2", at: Date.now() - 3600000, text: "naalaikku kaalaiyila call pannalaam", raw: "", language: "ta", mode: "tanglish", appName: "WhatsApp", audioMs: 3100, inferenceMs: 420, inserted: true },
];

const wait = <T>(v: T) => new Promise<T>((r) => setTimeout(() => r(v), 30));

export const mock: typeof Api = {
  getSettings: () => wait(settings),
  setSettings: (s) => wait((settings = s)),
  getModels: () => wait(models),
  downloadModel: async () => {},
  cancelDownload: async () => {},
  deleteModel: async () => {},
  inputDevices: () => wait(["MacBook Pro Microphone", "AirPods Pro"]),
  getOverlayState: async () => ({ state: { phase: "idle", message: null, mode: settings.outputMode }, target: { app: null, element: null }, settings }),
  toggleDictation: async () => {},
  commitDictation: async () => {},
  cancelDictation: async () => {},
  editDictation: async () => {},
  updateDraft: async () => {},
  insertDraft: async () => {},
  openHistory: async () => {},
  setOutputMode: async (mode) => void (settings = { ...settings, outputMode: mode }),
  overlayResize: async () => {},
  overlayDragged: async () => {},
  getPermissions: () => wait({ microphone: "granted", accessibility: "notDetermined" }),
  requestPermission: async () => {},
  openPermissionSettings: async () => {},
  getHistory: () => wait(history),
  deleteHistoryItem: async () => {},
  clearHistory: async () => {},
  polishStatus: () => wait({ reachable: false, models: [], error: "connection refused" }),
  copyText: async () => {},
  openMainWindow: async () => {},
  appInfo: () => wait({ version: "0.2.0", os: "macos", arch: "aarch64", modelsDir: "~/Library/Application Support/app.mozhi.desktop/models" }),
};
