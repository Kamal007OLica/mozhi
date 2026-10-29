// The floating dictation overlay. It never takes focus: the user's app keeps the caret,
// and the Rust key hook delivers ⌃⌃ / ↵ / esc. Buttons here still work (non-activating panel).
import "../shared/base.css";
import "./overlay.css";

import { Dots, type DotsState } from "../shared/dots";
import { h, fmtDuration } from "../shared/dom";
import { api, events, inTauri, MODE_LABELS, triggerGlyph, type OutputMode, type Phase, type Settings, type TargetPayload } from "../shared/ipc";
import { setLang, t } from "../shared/i18n";

const MODES: OutputMode[] = ["auto", "tamil", "tanglish", "english"];
const WINDOW_WIDTH = 560;
const SHADOW_ROOM = 36; // transparent margin so the card's shadow isn't clipped

let phase: Phase = "idle";
let mode: OutputMode = "auto";
let settings: Settings | null = null;
let startedAt = 0;
let text = "";

const dots = new Dots({ size: 10 });

const appIcon = h("img", { class: "target-icon", alt: "" }) as HTMLImageElement;
const appName = h("strong", { class: "target-app" });
const fieldName = h("span", { class: "target-field" });
const targetEl = h("div", { class: "target", "data-tauri-drag-region": true, title: "" }, appIcon, h("div", { class: "target-text", "data-tauri-drag-region": true }, h("span", { class: "eyebrow", "data-tauri-drag-region": true }), h("div", { class: "target-line", "data-tauri-drag-region": true }, appName, fieldName)));
const modeBtn = h("button", { class: "mode", type: "button", onclick: cycleMode });
const textEl = h("div", { class: "transcript", "aria-live": "polite" });
const statusEl = h("span", { class: "status" });
const timerEl = h("span", { class: "timer mono" });
const hintEl = h("span", { class: "hints" });
const cancelBtn = h("button", { class: "icon-btn", type: "button", "aria-label": "", onclick: () => api.cancelDictation() }, "✕");
const insertBtn = h("button", { class: "insert", type: "button", onclick: () => api.commitDictation() });

const card = h(
  "section",
  { class: "card", "data-phase": "idle" },
  h("header", { class: "head", "data-tauri-drag-region": true }, targetEl, modeBtn),
  textEl,
  h("footer", { class: "foot" }, h("div", { class: "voice" }, dots.el), h("div", { class: "state" }, statusEl, timerEl), h("div", { class: "actions" }, hintEl, cancelBtn, insertBtn)),
);
document.getElementById("overlay")!.append(card);

// ── rendering ────────────────────────────────────────────────────────────────

function applyLang() {
  setLang(settings?.uiLanguage ?? "ta");
  (targetEl.querySelector(".eyebrow") as HTMLElement).textContent = t("ov.into");
  cancelBtn.setAttribute("aria-label", t("ov.cancel"));
  cancelBtn.title = `${t("ov.cancel")} (esc)`;
  insertBtn.textContent = t("ov.insert");
  modeBtn.title = t("ov.mode");
  const key = triggerGlyph(settings?.hotkey.key ?? "control");
  hintEl.innerHTML = "";
  hintEl.append(h("kbd", null, `${key}${key}`), settings?.commitOnEnter !== false ? " / " : "", settings?.commitOnEnter !== false ? h("kbd", null, "↵") : "", ` ${t("ov.hintInsert")} · `, h("kbd", null, "esc"));
  renderMode();
  renderText();
}

function renderMode() {
  modeBtn.textContent = MODE_LABELS[mode].short;
  modeBtn.dataset.mode = mode;
}

function renderTarget(p: TargetPayload) {
  const app = p.app;
  targetEl.classList.toggle("warn", !app || app.isSelf);
  if (!app) {
    appIcon.hidden = true;
    appName.textContent = t("ov.noApp");
    fieldName.textContent = "";
    return;
  }
  if (app.isSelf) {
    appIcon.hidden = true;
    appName.textContent = t("ov.self");
    fieldName.textContent = "";
    return;
  }
  appIcon.hidden = !app.icon;
  if (app.icon) appIcon.src = app.icon;
  appName.textContent = app.name;
  const where = p.element?.label || (p.element?.editable ? null : null) || app.windowTitle;
  fieldName.textContent = where ? where : "";
  targetEl.title = [app.name, app.windowTitle, p.element?.label].filter(Boolean).join(" · ");
}

function renderText() {
  textEl.classList.toggle("empty", !text);
  textEl.textContent = text || t("ov.placeholder");
  textEl.lang = /[஀-௿]/.test(text) ? "ta" : "en";
  textEl.scrollTop = textEl.scrollHeight;
}

function renderPhase(next: Phase, message?: string | null) {
  phase = next;
  card.dataset.phase = next;
  const dotState: Record<Phase, DotsState> = {
    idle: "idle",
    starting: "idle",
    listening: "listening",
    transcribing: "transcribing",
    inserting: "transcribing",
    done: "done",
    error: "error",
  };
  dots.setState(dotState[next]);
  if (next !== "listening") dots.setLevel(0);

  const labels: Partial<Record<Phase, string>> = {
    starting: t("ov.starting"),
    listening: t("ov.listening"),
    transcribing: t("ov.transcribing"),
    inserting: t("ov.inserting"),
  };
  statusEl.textContent = message ?? labels[next] ?? "";
  if (next === "done" && message) statusEl.textContent = `✓ ${message}`;

  const live = next === "starting" || next === "listening";
  insertBtn.disabled = !live;
  cancelBtn.disabled = !(live || next === "transcribing");
  if (next === "starting") {
    startedAt = performance.now();
    text = "";
    renderText();
    chime("start");
  }
  if (next === "done") chime("done");
  if (next === "error") chime("error");
}

// ── behaviour ────────────────────────────────────────────────────────────────

function cycleMode() {
  mode = MODES[(MODES.indexOf(mode) + 1) % MODES.length];
  renderMode();
  api.setOutputMode(mode);
}

setInterval(() => {
  timerEl.textContent = phase === "listening" || phase === "starting" ? fmtDuration(performance.now() - startedAt) : "";
}, 250);

// Grow the native window with the card so nothing transparent sits over other apps.
new ResizeObserver(() => {
  if (!inTauri) return;
  const r = card.getBoundingClientRect();
  api.overlayResize(WINDOW_WIDTH, Math.ceil(r.height + SHADOW_ROOM)).catch(() => {});
}).observe(card);

// Remember where the user drags the overlay.
let dragging = false;
card.addEventListener("mousedown", (e) => {
  if ((e.target as HTMLElement).closest("[data-tauri-drag-region]")) dragging = true;
});
window.addEventListener("mouseup", () => {
  if (!dragging) return;
  dragging = false;
  setTimeout(() => api.overlayDragged().catch(() => {}), 120);
});

// Soft cues, synthesised — a wooden "tok" up, two notes down. Off when sounds are disabled.
let audio: AudioContext | null = null;
function chime(kind: "start" | "done" | "error") {
  if (!settings?.sounds) return;
  try {
    audio ??= new AudioContext();
    const notes = kind === "start" ? [660, 880] : kind === "done" ? [784, 523] : [220, 196];
    notes.forEach((f, i) => {
      const o = audio!.createOscillator();
      const g = audio!.createGain();
      const at = audio!.currentTime + i * 0.07;
      o.type = "sine";
      o.frequency.value = f;
      g.gain.setValueAtTime(0.0001, at);
      g.gain.exponentialRampToValueAtTime(0.09, at + 0.012);
      g.gain.exponentialRampToValueAtTime(0.0001, at + 0.16);
      o.connect(g).connect(audio!.destination);
      o.start(at);
      o.stop(at + 0.18);
    });
  } catch {
    /* audio is optional */
  }
}

// ── wiring ───────────────────────────────────────────────────────────────────

async function init() {
  if (!inTauri) return demo();
  const snap = await api.getOverlayState();
  settings = snap.settings;
  mode = snap.state.mode;
  applyLang();
  renderTarget(snap.target);
  renderPhase(snap.state.phase);

  events.onState((p) => {
    mode = p.mode;
    renderMode();
    renderPhase(p.phase, p.message);
  });
  events.onLevel((l) => dots.setLevel(l));
  events.onPartial((s) => {
    text = s;
    renderText();
  });
  events.onFinal((s) => {
    text = s;
    renderText();
  });
  events.onTarget(renderTarget);
  events.onSettings((s) => {
    settings = s;
    mode = s.outputMode;
    applyLang();
  });
}

/** Browser preview (`npm run dev` → /overlay.html): cycles through every state. */
function demo() {
  settings = { uiLanguage: "ta", sounds: false, commitOnEnter: true, hotkey: { key: "control", doubleTapMs: 380, maxTapMs: 260 } } as Settings;
  applyLang();
  renderTarget({ app: { name: "Slack", id: "com.tinyspeck.slackmacgap", pid: 1, windowTitle: "#general", icon: null, isSelf: false }, element: { role: "textArea", label: "Message #general", editable: true } });
  const words = "வணக்கம் எல்லாருக்கும், இன்னைக்கு meeting-க்கு கொஞ்சம் late ஆகும். பத்து நிமிஷத்துல வந்துடுறேன்.".split(" ");
  const run = () => {
    renderPhase("starting");
    setTimeout(() => renderPhase("listening"), 400);
    words.forEach((_, i) =>
      setTimeout(() => {
        text = words.slice(0, i + 1).join(" ");
        renderText();
      }, 900 + i * 420),
    );
    const end = 900 + words.length * 420 + 400;
    setTimeout(() => renderPhase("transcribing"), end);
    setTimeout(() => renderPhase("done", "Inserted into Slack"), end + 1600);
    setTimeout(run, end + 4200);
  };
  let t0 = 0;
  const wobble = (now: number) => {
    t0 = now / 1000;
    dots.setLevel(phase === "listening" ? 0.35 + 0.35 * Math.abs(Math.sin(t0 * 3.1) * Math.sin(t0 * 1.3)) : 0);
    requestAnimationFrame(wobble);
  };
  requestAnimationFrame(wobble);
  run();
}

init();
