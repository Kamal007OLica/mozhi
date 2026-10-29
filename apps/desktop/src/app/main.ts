// Mozhi's main window: home, models, writing, shortcut, history, permissions, about,
// plus first-run onboarding. Plain DOM; each page re-renders from `store` on change.
import "../shared/base.css";
import "./app.css";

import kolamUrl from "../../../../brand/patterns/kolam-7.svg";
import kolamInkUrl from "../../../../brand/patterns/kolam-9x5-ink.svg";
import vimanaUrl from "../../../../brand/patterns/vimana.svg";
import tigerUrl from "../../../../brand/patterns/muvendar-tiger.svg";
import bowUrl from "../../../../brand/patterns/muvendar-bow.svg";
import fishUrl from "../../../../brand/patterns/muvendar-fish.svg";

import { Dots, aythamMark } from "../shared/dots";
import { Voice } from "../shared/voice";
import { h, fmtBytes } from "../shared/dom";
import { getLang, setLang, t, tt, type Key } from "../shared/i18n";
import { mock } from "./mock";
import {
  api as realApi,
  events,
  inTauri,
  isMac,
  MODE_LABELS,
  triggerGlyph,
  type AppInfo,
  type DownloadProgress,
  type HistoryItem,
  type ModelInfo,
  type OutputMode,
  type PermissionKind,
  type Permissions,
  type PolishStatus,
  type Settings,
  type TriggerKey,
} from "../shared/ipc";

const api = inTauri ? realApi : mock;
const REPO_URL = "https://github.com/Kamal007OLica/mozhi";

type Page = "home" | "models" | "writing" | "shortcut" | "history" | "permissions" | "about";
const PAGES: { id: Page; key: Key }[] = [
  { id: "home", key: "nav.home" },
  { id: "models", key: "nav.models" },
  { id: "writing", key: "nav.writing" },
  { id: "shortcut", key: "nav.shortcut" },
  { id: "history", key: "nav.history" },
  { id: "permissions", key: "nav.permissions" },
  { id: "about", key: "nav.about" },
];

const store = {
  page: "home" as Page,
  settings: null as Settings | null,
  models: [] as ModelInfo[],
  perms: { microphone: "notRequired", accessibility: "notRequired" } as Permissions,
  history: [] as HistoryItem[],
  downloads: new Map<string, DownloadProgress>(),
  devices: [] as string[],
  polish: null as PolishStatus | null,
  info: null as AppInfo | null,
  query: "",
  onboardStep: 0,
};

const S = () => store.settings!;
const activeModelId = () => S().modelId || store.models.find((m) => m.recommended)?.id || "large-v3-turbo-q5";
const activeModel = () => store.models.find((m) => m.id === activeModelId());
const permOk = (s: string) => s === "granted" || s === "notRequired";
const ready = () => !!activeModel()?.installed && permOk(store.perms.microphone) && permOk(store.perms.accessibility);

async function save(patch: Partial<Settings>) {
  store.settings = await api.setSettings({ ...S(), ...patch });
  if (patch.uiLanguage) setLang(patch.uiLanguage);
  render();
}

let toastTimer = 0;
function toast(msg: string) {
  document.querySelector(".toast")?.remove();
  document.body.append(h("div", { class: "toast", role: "status" }, msg));
  clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => document.querySelector(".toast")?.remove(), 1800);
}

// ── building blocks ─────────────────────────────────────────────────────────

const pageHead = (n: number, key: Key, sub?: string) => {
  const { primary, secondary } = tt(key);
  return [
    h("div", { class: "eyebrow" }, `${String(n).padStart(2, "0")} · ${secondary}`),
    h("h1", { class: "page-title" }, primary),
    sub ? h("p", { class: "page-sub" }, sub) : null,
  ];
};

const sectionTitle = (key: Key) => {
  const { primary, secondary } = tt(key);
  return h("h2", { class: "section-title" }, primary, h("small", null, secondary));
};

const toggle = (checked: boolean, onChange: (v: boolean) => void, label: string) =>
  h("button", { class: "switch", role: "switch", "aria-checked": String(checked), "aria-label": label, onclick: () => onChange(!checked) });

const row = (key: Key, control: Node, hint?: string) => h("div", { class: "row" }, h("div", { class: "label" }, t(key), hint ? h("small", null, hint) : null), control);

const meter = (n: number, max = 5) => h("span", { class: "meter", "aria-label": `${n}/${max}` }, ...Array.from({ length: max }, (_, i) => h("i", { class: i < n ? "on" : "" })));

const sampleFor: Record<OutputMode, string> = {
  auto: "நாளைக்கு meeting இருக்கு / Meeting tomorrow",
  tamil: "நாளைக்கு காலையில meeting இருக்கு.",
  tanglish: "naalaikku kaalaiyila meeting irukku.",
  english: "There's a meeting tomorrow morning.",
};

function modeCards() {
  return h(
    "div",
    { class: "grid grid-4" },
    ...(Object.keys(MODE_LABELS) as OutputMode[]).map((m) => {
      const on = S().outputMode === m;
      return h(
        "button",
        { class: "card mode-card", "aria-pressed": String(on), onclick: () => api.setOutputMode(m).then(() => save({ outputMode: m })) },
        on ? h("span", { class: "tick", html: aythamMark(18, "var(--mz-mai)", "var(--mz-manjal)") }) : null,
        h("span", { class: "name", lang: "ta" }, MODE_LABELS[m].ta),
        h("span", { class: "en" }, MODE_LABELS[m].en.toUpperCase()),
        h("span", { class: "sample", lang: m === "english" || m === "tanglish" ? "en" : "ta" }, sampleFor[m]),
      );
    }),
  );
}

// ── pages ───────────────────────────────────────────────────────────────────

function homePage() {
  const key = triggerGlyph(S().hotkey.key);
  // The hero pill plays the dictation cycle on its own: dots → waveform → ஃ → done.
  const pillVoice = new Voice({ height: 22 });
  mounted.push(pillVoice);
  const cycle: [Parameters<Voice["setState"]>[0], number][] = [["idle", 1600], ["listening", 3200], ["transcribing", 1500], ["done", 1300]];
  let step = 0;
  let timer = 0;
  let wob = 0;
  const advance = () => {
    const [state, ms] = cycle[step++ % cycle.length];
    pillVoice.setState(state);
    timer = window.setTimeout(advance, ms);
  };
  const talk = () => {
    const t0 = performance.now() / 1000;
    pillVoice.setLevel(0.35 + 0.55 * Math.abs(Math.sin(t0 * 3.3) * Math.sin(t0 * 1.1)));
    wob = requestAnimationFrame(talk);
  };
  advance();
  talk();
  mounted.push({ destroy: () => (clearTimeout(timer), cancelAnimationFrame(wob)) });
  const words = store.history.reduce((n, i) => n + i.text.split(/\s+/).filter(Boolean).length, 0);
  const audioMin = store.history.reduce((n, i) => n + i.audioMs, 0) / 60000;
  const saved = Math.max(0, words / 40 - audioMin); // vs typing at 40 wpm

  const tryBox = h("textarea", { class: "try", placeholder: t("home.tryHint"), lang: "ta", spellcheck: false }) as HTMLTextAreaElement;
  tryArea = tryBox;

  const checks: [Key, boolean, string, () => void][] = [
    [
      "home.model",
      !!activeModel()?.installed,
      activeModel()?.name ?? "",
      () => go("models"),
    ],
    ["home.mic", permOk(store.perms.microphone), "", () => grant("microphone")],
    ["home.ax", permOk(store.perms.accessibility), "", () => grant("accessibility")],
  ];

  return [
    ...pageHead(1, "nav.home"),
    h(
      "section",
      { class: "card hero" },
      h("img", { class: "hero-art", src: kolamUrl, alt: "" }),
      h("div", { class: "keys" }, h("span", { class: "keycap" }, key), h("span", { class: "keycap" }, key), h("span", { class: "pill" }, pillVoice.el)),
      h("h2", { lang: getLang() }, t("home.title")),
      h("p", null, t("home.lede")),
      h(
        "div",
        { class: "hero-actions" },
        h("button", { class: "btn accent", onclick: () => api.toggleDictation() }, `${key}${key}  ${getLang() === "ta" ? "இப்போது பேசு" : "Dictate now"}`),
        h("span", { class: "segmented", role: "group", "aria-label": t("ov.mode") }, ...(Object.keys(MODE_LABELS) as OutputMode[]).map((m) => h("button", { "aria-pressed": String(S().outputMode === m), onclick: () => api.setOutputMode(m).then(() => save({ outputMode: m })) }, MODE_LABELS[m].short))),
      ),
    ),
    h(
      "section",
      { class: "card login-card" },
      h("span", { class: "login-mark", html: aythamMark(30, "var(--mz-manjal)", "var(--mz-mai)") }),
      h("div", { class: "label" }, h("strong", null, t("home.login")), h("small", null, t("home.loginHint"))),
      toggle(S().launchAtLogin, (v) => save({ launchAtLogin: v }), t("home.login")),
    ),
    ready()
      ? null
      : h(
          "section",
          { class: "card" },
          h("h2", { class: "section-title", style: "margin-top:0" }, t("home.setup")),
          ...checks.map(([k, ok, detail, fix]) =>
            h(
              "div",
              { class: `check ${ok ? "ok" : "todo"}` },
              h("span", { class: "mark" }, ok ? "✓" : "!"),
              h("div", { class: "label" }, t(k), detail ? h("small", null, detail) : null),
              ok ? null : h("button", { class: "btn small primary", onclick: fix }, k === "home.model" ? t("models.download") : t("perm.grant")),
            ),
          ),
        ),
    sectionTitle("home.try"),
    tryBox,
    store.history.length
      ? [
          sectionTitle("home.stats"),
          h(
            "section",
            { class: "card stats" },
            h("div", { class: "stat" }, h("b", null, words.toLocaleString()), h("span", null, t("home.words"))),
            h("div", { class: "stat" }, h("b", null, String(store.history.length)), h("span", null, t("home.dictations"))),
            h("div", { class: "stat" }, h("b", null, saved.toFixed(0)), h("span", null, t("home.saved"))),
          ),
        ]
      : null,
  ];
}

function modelCard(m: ModelInfo) {
  const dl = store.downloads.get(m.id);
  const active = m.id === activeModelId();
  const pct = dl && dl.total ? Math.round((dl.downloaded / dl.total) * 100) : 0;
  return h(
    "div",
    { class: "stamp-wrap" },
    h(
      "article",
      { class: `stamp ${m.installed ? "installed" : ""} ${active && m.installed ? "active" : ""}` },
      h(
        "div",
        { class: "stamp-inner" },
        m.recommended ? h("span", { class: "tag" }, t("models.recommended")) : null,
        h("h3", null, m.name),
        h("div", { class: "desc" }, m.description),
        h("div", { class: "meta" }, h("span", null, t("models.speed")), meter(m.speed)),
        h("div", { class: "meta" }, h("span", null, t("models.tamil")), meter(m.tamilQuality)),
        h("div", { class: "meta" }, h("span", null, fmtBytes(m.sizeMb * 1e6)), h("span", null, m.installed ? "✓" : "")),
        dl && !dl.done && !dl.error
          ? [h("div", { class: "progress" }, h("i", { style: `width:${pct}%` })), h("div", { class: "meta" }, h("span", null, `${fmtBytes(dl.downloaded)} / ${fmtBytes(dl.total)}`), h("button", { class: "btn small ghost", onclick: () => api.cancelDownload(m.id) }, t("models.cancel")))]
          : h(
              "div",
              { class: "stamp-actions" },
              m.installed
                ? [
                    active ? h("button", { class: "btn small primary", disabled: true }, t("models.inUse")) : h("button", { class: "btn small primary", onclick: () => save({ modelId: m.id }) }, t("models.use")),
                    h("button", { class: "btn small ghost danger", onclick: () => removeModel(m) }, t("models.delete")),
                  ]
                : h("button", { class: "btn small accent", onclick: () => download(m) }, `${t("models.download")} · ${fmtBytes(m.sizeMb * 1e6)}`),
              dl?.error ? h("small", { style: "color:var(--mz-kumkum-700)" }, dl.error) : null,
            ),
      ),
    ),
  );
}

function modelsPage() {
  return [
    ...pageHead(2, "models.title", t("models.lede")),
    h("div", { class: "grid grid-2" }, ...store.models.map(modelCard)),
    h("p", { class: "page-sub", style: "margin-top:24px" }, t("models.custom"), store.info ? h("span", { class: "mono", style: "display:block" }, store.info.modelsDir) : null),
  ];
}

function writingPage() {
  const s = S();
  const endpoint = h("input", { class: "field", value: s.polish.endpoint, spellcheck: false, onchange: (e: Event) => save({ polish: { ...s.polish, endpoint: (e.target as HTMLInputElement).value.trim() } }) });
  const llmOptions = Array.from(new Set([s.polish.model, ...(store.polish?.models ?? [])]));
  const llm = h(
    "select",
    { class: "field", onchange: (e: Event) => save({ polish: { ...s.polish, model: (e.target as HTMLSelectElement).value } }) },
    ...llmOptions.map((m) => h("option", { value: m, selected: m === s.polish.model }, m)),
  );
  const polishState = store.polish
    ? store.polish.reachable
      ? `✓ Ollama · ${store.polish.models.length} models`
      : `✕ ${store.polish.error ?? "unreachable"} · ollama.com`
    : "";
  const mic = h(
    "select",
    { class: "field", onchange: (e: Event) => save({ micDevice: (e.target as HTMLSelectElement).value || null }) },
    h("option", { value: "" }, t("writing.micDefault")),
    ...store.devices.map((d) => h("option", { value: d, selected: d === s.micDevice }, d)),
  );

  return [
    ...pageHead(3, "writing.title"),
    modeCards(),
    sectionTitle("writing.polish"),
    h(
      "section",
      { class: "card" },
      h("p", { style: "margin:0 0 6px;color:var(--mz-ink-muted)" }, t("writing.polishLede")),
      row("writing.polish", toggle(s.polish.enabled, (v) => save({ polish: { ...s.polish, enabled: v } }), t("writing.polish"))),
      row("writing.endpoint", endpoint),
      row("writing.llm", llm, polishState),
      h("div", { class: "row" }, h("div", { class: "label" }, h("small", null, getLang() === "ta" ? "பரிந்துரை: ollama pull gemma3:4b" : "Suggested: ollama pull gemma3:4b")), h("button", { class: "btn small", onclick: checkPolish }, t("writing.test"))),
    ),
    sectionTitle("writing.behaviour"),
    h(
      "section",
      { class: "card" },
      row("writing.mic", mic),
      row("writing.livePreview", toggle(s.livePreview, (v) => save({ livePreview: v }), t("writing.livePreview"))),
      row("writing.autoPaste", toggle(s.autoPaste, (v) => save({ autoPaste: v }), t("writing.autoPaste"))),
      row("writing.restore", toggle(s.restoreClipboard, (v) => save({ restoreClipboard: v }), t("writing.restore"))),
      row("writing.sounds", toggle(s.sounds, (v) => save({ sounds: v }), t("writing.sounds"))),
      row("writing.history", toggle(s.historyEnabled, (v) => save({ historyEnabled: v }), t("writing.history"))),
    ),
  ];
}

function shortcutPage() {
  const s = S();
  const keys: TriggerKey[] = isMac ? ["control", "rightControl", "option", "rightOption", "fn"] : ["control", "rightControl", "option", "rightOption"];
  const gap = h("input", {
    type: "range",
    min: "250",
    max: "600",
    step: "10",
    value: String(s.hotkey.doubleTapMs),
    "aria-label": t("shortcut.speed"),
    onchange: (e: Event) => save({ hotkey: { ...s.hotkey, doubleTapMs: Number((e.target as HTMLInputElement).value) } }),
    oninput: (e: Event) => ((e.target as HTMLElement).nextElementSibling!.textContent = `${(e.target as HTMLInputElement).value} ms`),
  });
  return [
    ...pageHead(4, "shortcut.title", t("shortcut.lede")),
    h(
      "section",
      { class: "card" },
      h("div", { class: "segmented", role: "group", "aria-label": t("shortcut.title") }, ...keys.map((k) => h("button", { "aria-pressed": String(s.hotkey.key === k), onclick: () => save({ hotkey: { ...s.hotkey, key: k } }) }, `${triggerGlyph(k)} ${triggerGlyph(k)}`))),
      h("div", { class: "row", style: "margin-top:12px" }, h("div", { class: "label" }, t("shortcut.speed")), gap, h("span", { class: "mono" }, `${s.hotkey.doubleTapMs} ms`)),
      row("shortcut.enter", toggle(s.commitOnEnter, (v) => save({ commitOnEnter: v }), t("shortcut.enter"))),
      row("shortcut.position", h("button", { class: "btn small", disabled: !s.overlayPosition, onclick: () => save({ overlayPosition: null }) }, t("shortcut.resetPos"))),
    ),
    h("section", { class: "card", style: "margin-top:16px" }, h("p", { style: "margin:0" }, h("kbd", null, `${triggerGlyph(s.hotkey.key)}${triggerGlyph(s.hotkey.key)}`), "  ", getLang() === "ta" ? "தொடங்க / இட" : "start / insert", "   ·   ", h("kbd", null, "↵"), "  ", getLang() === "ta" ? "இட" : "insert", "   ·   ", h("kbd", null, "esc"), "  ", getLang() === "ta" ? "ரத்து" : "cancel")),
  ];
}

let confirmClear = false;
function historyPage() {
  const q = store.query.toLowerCase();
  const items = store.history.filter((i) => !q || i.text.toLowerCase().includes(q) || (i.appName ?? "").toLowerCase().includes(q));
  const search = h("input", {
    class: "field",
    type: "search",
    placeholder: t("history.search"),
    value: store.query,
    oninput: (e: Event) => {
      store.query = (e.target as HTMLInputElement).value;
      const list = document.getElementById("history-list");
      list?.replaceWith(historyList(store.history.filter((i) => !store.query || i.text.toLowerCase().includes(store.query.toLowerCase()))));
    },
  });
  return [
    ...pageHead(5, "history.title", t("history.local")),
    store.history.length
      ? h(
          "div",
          { class: "row", style: "border:0;padding-top:0" },
          search,
          h("span", { style: "flex:1" }),
          h(
            "button",
            {
              class: "btn small danger",
              onclick: async () => {
                if (!confirmClear) {
                  confirmClear = true;
                  render();
                  setTimeout(() => ((confirmClear = false), render()), 3000);
                  return;
                }
                confirmClear = false;
                await api.clearHistory();
                await refreshHistory();
              },
            },
            confirmClear ? t("history.confirmClear") : t("history.clear"),
          ),
        )
      : null,
    historyList(items),
  ];
}

function historyList(items: HistoryItem[]) {
  if (!store.history.length) return h("div", { id: "history-list", class: "empty" }, h("img", { src: kolamInkUrl, alt: "" }), h("p", null, t("history.empty")));
  return h(
    "div",
    { id: "history-list" },
    ...items.map((i) => {
      const d = new Date(i.at);
      return h(
        "article",
        { class: "receipt" },
        h("div", { class: "receipt-head" }, h("span", null, `${d.toLocaleDateString()} ${d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}`), h("span", null, `${i.appName ?? t("hist.copied")} · ${MODE_LABELS[i.mode].short}`)),
        h("div", { class: "receipt-text", lang: i.language === "en" ? "en" : "ta" }, i.text),
        h(
          "div",
          { class: "receipt-foot" },
          h("span", null, `${(i.audioMs / 1000).toFixed(1)}s audio · ${i.inferenceMs} ms · ${i.inserted ? "✓ inserted" : "copied"}`),
          h(
            "span",
            null,
            h("button", { class: "btn small ghost", onclick: () => api.copyText(i.text).then(() => toast(t("history.copied"))) }, t("history.copy")),
            h("button", { class: "btn small ghost danger", "aria-label": t("models.delete"), onclick: () => api.deleteHistoryItem(i.id).then(refreshHistory) }, "✕"),
          ),
        ),
      );
    }),
  );
}

function permissionsPage() {
  const card = (kind: PermissionKind, title: Key, why: Key) => {
    const state = store.perms[kind];
    const ok = permOk(state);
    return h(
      "section",
      { class: "card" },
      h(
        "div",
        { class: `check ${ok ? "ok" : "todo"}`, style: "border:0" },
        h("span", { class: "mark" }, ok ? "✓" : "!"),
        h("div", { class: "label" }, h("strong", null, t(title)), h("small", null, t(why))),
        ok ? h("span", { class: "mono" }, state === "notRequired" ? t("perm.notNeeded") : t("perm.granted")) : [h("button", { class: "btn small primary", onclick: () => grant(kind) }, t("perm.grant")), h("button", { class: "btn small ghost", onclick: () => api.openPermissionSettings(kind) }, t("perm.open"))],
      ),
    );
  };
  return [...pageHead(6, "perm.title"), card("microphone", "perm.mic", "perm.micWhy"), card("accessibility", "perm.ax", "perm.axWhy")];
}

function aboutPage() {
  return [
    ...pageHead(7, "about.title", t("about.story")),
    h("div", { class: "emblems" }, h("img", { src: bowUrl, alt: "Chera bow" }), h("span", { html: aythamMark(96, "var(--mz-mai)") }), h("img", { src: tigerUrl, alt: "Chola tiger", style: "height:72px" }), h("img", { src: fishUrl, alt: "Pandya fish" })),
    h("p", { class: "kural", lang: "ta" }, "அகர முதல எழுத்தெல்லாம் ஆதி", h("br"), "பகவன் முதற்றே உலகு"),
    h("p", { class: "kural-src" }, "திருக்குறள் · 1"),
    h(
      "section",
      { class: "card", style: "margin-top:28px" },
      row("about.open" as Key, h("a", { class: "btn small", href: REPO_URL, target: "_blank", rel: "noreferrer" }, "GitHub ↗")),
      h("div", { class: "row" }, h("div", { class: "label" }, t("about.private"))),
      store.info ? h("div", { class: "row" }, h("div", { class: "label mono" }, `Mozhi ${store.info.version} · ${store.info.os}/${store.info.arch}`)) : null,
    ),
  ];
}

// ── onboarding ──────────────────────────────────────────────────────────────

function onboarding() {
  const step = store.onboardStep;
  const next = () => {
    store.onboardStep++;
    render();
  };
  const finish = () => save({ onboarded: true });
  const rec = store.models.find((m) => m.recommended) ?? store.models[0];
  const steps = [
    () => [h("h1", { class: "page-title" }, t("ob.welcome")), h("p", { class: "page-sub" }, t("ob.welcomeLede")), h("p", { class: "page-sub" }, t("about.private"))],
    () => [h("div", { class: "eyebrow" }, `01 · ${t("ob.step1")}`), ...permissionsPage().slice(1)],
    () => [h("div", { class: "eyebrow" }, `02 · ${t("ob.step2")}`), h("h1", { class: "page-title" }, t("models.title")), h("p", { class: "page-sub" }, t("models.lede")), rec ? modelCard(rec) : null],
    () => [h("div", { class: "eyebrow" }, `03 · ${t("ob.step3")}`), h("h1", { class: "page-title" }, t("home.title")), h("p", { class: "page-sub" }, t("home.lede")), modeCards()],
  ];
  const last = step === steps.length - 1;
  return h(
    "div",
    { class: "onboard", role: "dialog", "aria-modal": "true" },
    h("div", { class: "onboard-art" }, h("img", { class: "kolam", src: kolamUrl, alt: "" }), h("div", { class: "big", lang: "ta" }, "மொ", h("em", null, "ழி"))),
    h(
      "div",
      { class: "onboard-body" },
      h("div", { class: "steps" }, ...steps.map((_, i) => h("i", { class: i <= step ? "on" : "" }))),
      ...steps[step](),
      h(
        "div",
        { class: "onboard-actions" },
        step > 0 && !last ? h("button", { class: "btn ghost", onclick: next }, t("ob.skip")) : null,
        h("button", { class: "btn primary", onclick: last ? finish : next }, step === 0 ? t("ob.start") : last ? t("ob.done") : t("ob.next")),
      ),
    ),
  );
}

// Asked once, after onboarding: start Mozhi at login so the double-tap is always ready.
const LOGIN_ASKED = "mozhi.loginPrompted";
const loginAsked = () => {
  try {
    return localStorage.getItem(LOGIN_ASKED) === "1";
  } catch {
    return true;
  }
};
function loginPrompt() {
  const close = (enable: boolean) => {
    try {
      localStorage.setItem(LOGIN_ASKED, "1");
    } catch {
      /* the toggle on Home still works */
    }
    if (enable) save({ launchAtLogin: true });
    else render();
  };
  const voice = new Voice({ height: 30 });
  mounted.push(voice);
  voice.setState("done");
  return h(
    "div",
    { class: "modal-backdrop" },
    h(
      "div",
      { class: "modal", role: "dialog", "aria-modal": "true", "aria-labelledby": "login-title" },
      h("img", { class: "modal-kolam", src: kolamUrl, alt: "" }),
      h("div", { class: "modal-voice" }, voice.el),
      h("h2", { id: "login-title" }, t("login.title")),
      h("p", null, t("login.body")),
      h(
        "div",
        { class: "modal-actions" },
        h("button", { class: "btn ghost", onclick: () => close(false) }, t("login.no")),
        h("button", { class: "btn primary", autofocus: true, onclick: () => close(true) }, t("login.yes")),
      ),
    ),
  );
}

// ── actions ─────────────────────────────────────────────────────────────────

async function download(m: ModelInfo) {
  store.downloads.set(m.id, { id: m.id, downloaded: 0, total: m.sizeMb * 1e6, done: false, error: null });
  render();
  try {
    await api.downloadModel(m.id);
  } catch (e) {
    store.downloads.set(m.id, { id: m.id, downloaded: 0, total: 0, done: false, error: String(e) });
  }
  store.models = await api.getModels();
  if (!S().modelId) await save({ modelId: m.id });
  render();
}

async function removeModel(m: ModelInfo) {
  await api.deleteModel(m.id);
  store.models = await api.getModels();
  render();
}

async function grant(kind: PermissionKind) {
  await api.requestPermission(kind);
  // System dialogs are async and outside our window — poll briefly for the answer.
  for (let i = 0; i < 20; i++) {
    await new Promise((r) => setTimeout(r, 700));
    store.perms = await api.getPermissions();
    if (permOk(store.perms[kind])) break;
  }
  render();
}

async function checkPolish() {
  store.polish = await api.polishStatus(S().polish);
  render();
}

async function refreshHistory() {
  store.history = await api.getHistory();
  render();
}

function go(page: Page) {
  store.page = page;
  render();
  document.querySelector(".main")?.scrollTo({ top: 0 });
}

// ── render ──────────────────────────────────────────────────────────────────

let mounted: { destroy(): void }[] = [];
let tryArea: HTMLTextAreaElement | null = null;
const statusDots = new Dots({ size: 7, ring: false });

function render() {
  if (!store.settings) return;
  const active = document.activeElement as HTMLElement | null;
  const keepFocus = active?.matches("input[type=search], textarea") ? { sel: active.matches("textarea") ? "textarea.try" : "input[type=search]", start: (active as HTMLInputElement).selectionStart, val: (active as HTMLInputElement).value } : null;

  mounted.forEach((d) => d.destroy());
  mounted = [];
  tryArea = null;

  const pages: Record<Page, () => (Node | null | (Node | null)[])[]> = {
    home: homePage,
    models: modelsPage,
    writing: writingPage,
    shortcut: shortcutPage,
    history: historyPage,
    permissions: permissionsPage,
    about: aboutPage,
  };
  const needsAttention = (p: Page) => (p === "models" && !activeModel()?.installed) || (p === "permissions" && !(permOk(store.perms.microphone) && permOk(store.perms.accessibility)));

  const app = document.getElementById("app")!;
  app.replaceChildren(
    h(
      "div",
      { class: "shell" },
      h(
        "aside",
        { class: "side" },
        h("div", { class: "drag", "data-tauri-drag-region": true }),
        h("img", { class: "side-art", src: vimanaUrl, alt: "" }),
        h("div", { class: "brand" }, h("span", { html: aythamMark(40) }), h("div", null, h("div", { class: "wordmark", lang: "ta" }, "மொ", h("em", null, "ழி")), h("span", { class: "wordmark-sub" }, "MOZHI"))),
        h(
          "nav",
          { class: "nav", "aria-label": "Mozhi" },
          ...PAGES.map(({ id, key }) => {
            const { primary, secondary } = tt(key);
            return h("button", { "aria-current": store.page === id ? "page" : false, onclick: () => go(id) }, h("span", { class: "primary" }, primary), needsAttention(id) ? h("span", { class: "badge", title: "!" }) : h("span", { class: "secondary" }, secondary));
          }),
        ),
        h(
          "div",
          { class: "side-foot" },
          h("div", { class: "status-pill" }, h("span", { class: "dots-slot" }, statusDots.el), ready() ? t("home.ready") : t("home.setup")),
          h("button", { class: "lang-toggle", onclick: () => save({ uiLanguage: getLang() === "ta" ? "en" : "ta" }) }, t("common.langToggle")),
        ),
      ),
      h("main", { class: "main" }, h("div", { class: "page", "data-page": store.page }, ...pages[store.page]().flat())),
    ),
    store.settings.onboarded ? "" : onboarding(),
    store.settings.onboarded && !store.settings.launchAtLogin && !loginAsked() ? loginPrompt() : "",
  );
  (document.querySelector(".modal .btn.primary") as HTMLButtonElement | null)?.focus();

  if (keepFocus) {
    const el = document.querySelector(keepFocus.sel) as HTMLInputElement | null;
    if (el) {
      el.value = keepFocus.val;
      el.focus();
      if (keepFocus.start !== null) el.setSelectionRange(keepFocus.start, keepFocus.start);
    }
  }
}

// ── boot ────────────────────────────────────────────────────────────────────

async function boot() {
  const [settings, models, perms, history, devices, info] = await Promise.all([api.getSettings(), api.getModels(), api.getPermissions(), api.getHistory(), api.inputDevices(), api.appInfo()]);
  Object.assign(store, { settings, models, perms, history, devices, info });
  setLang(settings.uiLanguage);
  render();
  if (settings.polish.enabled) checkPolish();

  events.onSettings((s) => {
    store.settings = s;
    setLang(s.uiLanguage);
    render();
  });
  events.onHistory(refreshHistory);
  events.onNavigate((p) => go(p as Page));
  events.onDownload((p) => {
    if (p.done) store.downloads.delete(p.id);
    else store.downloads.set(p.id, p);
    // progress updates only touch the models views
    if (store.page === "models" || !store.settings?.onboarded) render();
  });
  // Text dictated while Mozhi itself is focused lands in the try box.
  events.onFinal((text) => {
    if (tryArea && document.hasFocus() && document.activeElement === tryArea) {
      tryArea.setRangeText(text + " ", tryArea.selectionStart, tryArea.selectionEnd, "end");
    }
  });
  events.onState((p) => statusDots.setState(p.phase === "listening" ? "listening" : p.phase === "transcribing" ? "transcribing" : "idle"));
  events.onLevel((l) => statusDots.setLevel(l));
  // Permissions can change in System Settings while we're open.
  window.addEventListener("focus", async () => {
    store.perms = await api.getPermissions();
    render();
  });
}

boot();
