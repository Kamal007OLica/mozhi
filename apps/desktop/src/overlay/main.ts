// The floating dictation overlay. While listening it never takes focus: the user's app keeps
// the caret, and the Rust key hook delivers ⌃⌃ / ↵ / esc. Buttons still work (non-activating
// panel). Clicking the transcript switches to editing: the panel takes keyboard focus (on
// macOS without activating Mozhi), ↵ inserts the edited text into the user's app.
import "../shared/base.css";
import "./overlay.css";

import { Voice, type VoiceState } from "../shared/voice";
import { h, fmtDuration } from "../shared/dom";
import { api, events, inTauri, MODE_LABELS, triggerGlyph, type OutputMode, type Phase, type Settings, type TargetPayload } from "../shared/ipc";
import { setLang, t } from "../shared/i18n";

const MODES: OutputMode[] = ["auto", "tamil", "tanglish", "english"];
const WINDOW_WIDTH = 600;
const GLOW_ROOM = 64; // transparent margin above and below the card for its shadow and aura

let phase: Phase = "idle";
let mode: OutputMode = "auto";
let settings: Settings | null = null;
let startedAt = 0;
let text = "";

const stage = h("div", { class: "stage" });
const voice = new Voice({ height: 30, onLevel: (l) => stage.style.setProperty("--lvl", l.toFixed(3)) });

const appIcon = h("img", { class: "target-icon", alt: "" }) as HTMLImageElement;
const appName = h("strong", { class: "target-app" });
const fieldName = h("span", { class: "target-field" });
const targetEl = h("div", { class: "target", "data-tauri-drag-region": true, title: "" }, appIcon, h("div", { class: "target-text", "data-tauri-drag-region": true }, h("span", { class: "eyebrow", "data-tauri-drag-region": true }), h("div", { class: "target-line", "data-tauri-drag-region": true }, appName, fieldName)));
const modeBtn = h("button", { class: "mode", type: "button", onclick: cycleMode });
const LINES = 3;
const textEl = h("textarea", { class: "transcript", rows: 1, readonly: true, spellcheck: "false", autocomplete: "off", "aria-live": "polite" }) as HTMLTextAreaElement;
const statusEl = h("span", { class: "status" });
const timerEl = h("span", { class: "timer mono" });
const hintEl = h("span", { class: "hints" });
const cancelBtn = h("button", { class: "icon-btn", type: "button", "aria-label": "", onclick: () => api.cancelDictation() }, "✕");
const historyBtn = h("button", {
  class: "icon-btn history-btn",
  type: "button",
  "aria-label": "",
  onclick: () => api.openHistory(),
  html: `<svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="5" y="4" width="14" height="17" rx="2.5"/><path d="M9 4.5V3.6c0-.6.4-1 1-1h4c.6 0 1 .4 1 1v.9M9 10h6M9 14h6M9 18h3.5"/></svg>`,
});
const insertBtn = h("button", { class: "insert", type: "button", onclick: insertNow });

const card = h(
  "section",
  { class: "card", "data-phase": "idle" },
  h("i", { class: "edge", "aria-hidden": "true" }),
  h("i", { class: "shine", "aria-hidden": "true" }),
  h("header", { class: "head", "data-tauri-drag-region": true }, targetEl, modeBtn),
  textEl,
  h("footer", { class: "foot" }, h("div", { class: "well" }, voice.el), h("div", { class: "state" }, statusEl, timerEl), h("div", { class: "actions" }, hintEl, historyBtn, cancelBtn, insertBtn)),
);
stage.append(h("i", { class: "aura", "aria-hidden": "true" }), card);
document.getElementById("overlay")!.append(stage);

// ── rendering ────────────────────────────────────────────────────────────────

function applyLang() {
  setLang(settings?.uiLanguage ?? "ta");
  (targetEl.querySelector(".eyebrow") as HTMLElement).textContent = t("ov.into");
  cancelBtn.setAttribute("aria-label", t("ov.cancel"));
  cancelBtn.title = `${t("ov.cancel")} (esc)`;
  historyBtn.setAttribute("aria-label", t("ov.history"));
  historyBtn.title = t("ov.history");
  textEl.placeholder = t("ov.placeholder");
  insertBtn.textContent = t("ov.insert");
  modeBtn.title = t("ov.mode");
  renderHints();
  renderMode();
  renderText();
}

function renderHints() {
  const key = triggerGlyph(settings?.hotkey.key ?? "control");
  hintEl.innerHTML = "";
  if (phase === "editing") {
    hintEl.append(
      h("span", { class: "hint" }, h("kbd", null, "↵"), t("ov.hintInsert")),
      h("span", { class: "hint" }, h("kbd", null, "⇧↵"), t("ov.hintNewline")),
    );
    return;
  }
  hintEl.append(
    h("span", { class: "hint" }, h("kbd", null, `${key}${key}`), settings?.commitOnEnter !== false ? h("kbd", null, "↵") : "", t("ov.hintInsert")),
    h("span", { class: "hint" }, h("kbd", null, "esc"), t("ov.hintCancel")),
  );
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

/** At most three lines tall. While listening the newest words stay in view and older ones
 *  fade out at the top; while editing the box scrolls with the caret. */
function fitText() {
  const cs = getComputedStyle(textEl);
  const line = parseFloat(cs.lineHeight) || 28;
  const pad = parseFloat(cs.paddingTop) + parseFloat(cs.paddingBottom);
  textEl.style.height = "auto";
  const full = textEl.scrollHeight;
  const max = line * LINES + pad;
  textEl.style.height = `${Math.min(full, max)}px`;
  markClipped();
}

/** Fade the top edge only while there is text hidden above it. */
function markClipped() {
  textEl.classList.toggle("clipped", textEl.scrollTop > 2);
}

function renderText() {
  // Never overwrite what the user is typing.
  if (phase === "editing" && document.activeElement === textEl) return;
  textEl.classList.toggle("empty", !text);
  textEl.value = text;
  textEl.lang = /[஀-௿]/.test(text) ? "ta" : "en";
  fitText();
  textEl.scrollTop = textEl.scrollHeight;
  markClipped();
}

function insertNow() {
  if (phase === "editing") api.insertDraft(textEl.value);
  else api.commitDictation();
}

function beginEditing() {
  textEl.readOnly = false;
  // The native panel becomes key a moment after the phase change; focus once it has.
  const focus = () => {
    textEl.focus({ preventScroll: true });
    const end = textEl.value.length;
    textEl.setSelectionRange(end, end);
    textEl.scrollTop = textEl.scrollHeight;
  };
  focus();
  setTimeout(focus, 60);
  setTimeout(focus, 200);
}

function renderPhase(next: Phase, message?: string | null) {
  phase = next;
  card.dataset.phase = next;
  stage.dataset.phase = next;
  const voiceState: Record<Phase, VoiceState> = {
    idle: "idle",
    starting: "idle",
    listening: "listening",
    transcribing: "transcribing",
    editing: "idle",
    inserting: "transcribing",
    done: "done",
    error: "error",
  };
  voice.setState(voiceState[next]);
  if (next !== "listening") voice.setLevel(0);

  const labels: Partial<Record<Phase, string>> = {
    starting: t("ov.starting"),
    listening: t("ov.listening"),
    transcribing: t("ov.transcribing"),
    inserting: t("ov.inserting"),
  };
  statusEl.textContent = message ?? labels[next] ?? "";
  if (next === "done" && message) statusEl.textContent = `✓ ${message}`;

  const live = next === "starting" || next === "listening";
  const editing = next === "editing";
  insertBtn.disabled = !(live || editing);
  cancelBtn.disabled = !(live || editing || next === "transcribing");
  historyBtn.disabled = next === "idle" || next === "transcribing" || next === "inserting";
  textEl.title = live ? t("ov.editHint") : "";
  if (editing) beginEditing();
  else {
    textEl.readOnly = true;
    if (document.activeElement === textEl) textEl.blur();
  }
  renderHints();
  if (editing) statusEl.textContent = t("ov.editing");
  if (next === "starting") {
    // replay the entrance each time the pill appears
    card.classList.remove("enter");
    void card.offsetWidth;
    card.classList.add("enter");
    startedAt = performance.now();
    text = "";
    renderText();
    chime("start");
  }
  if (next === "done") chime("done");
  if (next === "error") chime("error");
}

// ── behaviour ────────────────────────────────────────────────────────────────

// Click into the live transcript → stop listening and edit it.
textEl.addEventListener("mousedown", (e) => {
  if (phase === "starting" || phase === "listening") {
    e.preventDefault();
    api.editDictation();
  }
});

textEl.addEventListener("scroll", markClipped, { passive: true });
// Tamil glyphs are taller than the fallback font: measure again once the web fonts are in.
document.fonts?.ready.then(() => {
  fitText();
  if (phase !== "editing") textEl.scrollTop = textEl.scrollHeight;
  markClipped();
});

let draftTimer = 0;
textEl.addEventListener("input", () => {
  text = textEl.value;
  textEl.classList.toggle("empty", !text);
  fitText();
  clearTimeout(draftTimer);
  draftTimer = window.setTimeout(() => api.updateDraft(textEl.value).catch(() => {}), 40);
});

textEl.addEventListener("keydown", (e) => {
  if (phase !== "editing" || e.isComposing || e.keyCode === 229) return; // let Tamil IMEs finish their composition
  if (e.key === "Enter" && !e.shiftKey && !e.altKey) {
    e.preventDefault();
    api.insertDraft(textEl.value);
  } else if (e.key === "Escape") {
    e.preventDefault();
    api.cancelDictation();
  }
});

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
  api.overlayResize(WINDOW_WIDTH, Math.ceil(r.height + GLOW_ROOM)).catch(() => {});
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
  events.onLevel((l) => voice.setLevel(l));
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
  const words = "வணக்கம் எல்லாருக்கும், இன்னைக்கு meeting-க்கு கொஞ்சம் late ஆகும். பத்து நிமிஷத்துல வந்துடுறேன். அதுக்குள்ள agenda-வ ஒரு தடவை பாத்துருங்க, அப்புறம் client call-ல என்ன பேசணும்னு முடிவு பண்ணலாம்.".split(" ");
  // `?phase=editing` (or any phase) freezes the preview in that state for screenshots.
  const frozen = new URLSearchParams(location.search).get("phase") as Phase | null;
  if (frozen) {
    text = words.join(" ");
    renderPhase(frozen === "editing" ? "listening" : frozen, frozen === "done" ? "Inserted into Slack" : null);
    renderText();
    if (frozen === "editing") renderPhase("editing");
    return;
  }
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
    setTimeout(() => renderPhase("editing"), end + 1200);
    setTimeout(() => renderPhase("done", "Inserted into Slack"), end + 7000);
    setTimeout(run, end + 9600);
  };
  let t0 = 0;
  const wobble = (now: number) => {
    t0 = now / 1000;
    voice.setLevel(phase === "listening" ? 0.3 + 0.6 * Math.abs(Math.sin(t0 * 3.1) * Math.sin(t0 * 1.3)) : 0);
    requestAnimationFrame(wobble);
  };
  requestAnimationFrame(wobble);
  run();
}

init();
