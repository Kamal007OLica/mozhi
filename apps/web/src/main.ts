import "./fonts.css";
import "./tokens.css";
import "./styles.css";
import { AythamDots, type DotPhase } from "./dots";

// ── Hero demo: ⌃⌃ → the pill names the target app → words arrive → inserted ──

interface Scene {
  app: string;
  icon: string;
  iconBg: string;
  field: string;
  title: string;
  mode: string;
  lang: "ta" | "en";
  /** [author, message, language, English meaning (when the message isn't English)] */
  thread: [string, string, "ta" | "en", string?][];
  words: string[];
  /** What the dictated sentence means, for readers who don't read Tamil. */
  gloss?: string;
}

const SCENES: Scene[] = [
  {
    app: "Slack",
    icon: "S",
    iconBg: "#4a154b",
    field: "#general · message",
    title: "Slack · #general",
    mode: "தமிழ்",
    lang: "ta",
    thread: [["Priya", "நாளைக்கு standup எப்போ வெக்கலாம்?", "ta", "When shall we do standup tomorrow?"]],
    words: ["நாளைக்கு", "காலையில", "meeting-அ", "பத்து", "மணிக்கு", "வெச்சுக்கலாமா?"],
    gloss: "Shall we keep the meeting at ten tomorrow morning?",
  },
  {
    app: "WhatsApp",
    icon: "W",
    iconBg: "#1f8f4e",
    field: "Amma · message",
    title: "WhatsApp · Amma",
    mode: "Tanglish",
    lang: "en",
    thread: [["Amma", "Eppo da varuva? 🙂", "en", "When are you coming, dear?"]],
    words: ["Amma,", "naan", "Sunday", "kaalaila", "Chennai-ku", "varen.", "Ticket", "book", "pannitten."],
    gloss: "Mum, I'm coming to Chennai on Sunday morning. I've booked the ticket.",
  },
  {
    app: "Notes",
    icon: "N",
    iconBg: "#c4960f",
    field: "Thanjavur trip",
    title: "Notes · Thanjavur trip",
    mode: "English",
    lang: "en",
    thread: [["Notes", "Day 2:", "en"]],
    words: ["Visit", "the", "Big", "Temple", "before", "sunset", "and", "photograph", "the", "inscriptions."],
  },
];

function $(root: ParentNode, key: string) {
  const el = root.querySelector<HTMLElement>(`[data-demo="${key}"]`);
  if (!el) throw new Error(`demo: missing ${key}`);
  return el;
}

function runDemo(root: HTMLElement) {
  const ui = {
    title: $(root, "title"),
    // The English subtitle sits just below the demo stage.
    gloss: $(root.parentElement ?? root, "gloss"),
    glossText: $(root.parentElement ?? root, "glosstext"),
    thread: $(root, "thread"),
    composed: $(root, "composed"),
    composer: $(root, "composer"),
    key: $(root, "key"),
    keylabel: $(root, "keylabel"),
    overlay: $(root, "overlay"),
    icon: $(root, "icon"),
    app: $(root, "app"),
    field: $(root, "field"),
    mode: $(root, "mode"),
    text: $(root, "text"),
    statusTa: $(root, "status-ta"),
    statusEn: $(root, "status-en"),
  };
  const dots = new AythamDots(root.querySelector<SVGSVGElement>('[data-demo="dots"]')!);

  // A clock that only advances while the demo is on screen and the tab is visible.
  let visible = false;
  let clock = 0;
  let last = performance.now();
  const waiters: { at: number; resolve: () => void }[] = [];
  const tick = (now: number) => {
    if (visible && !document.hidden) clock += Math.min(250, now - last);
    last = now;
    for (let i = waiters.length - 1; i >= 0; i--) {
      if (clock >= waiters[i].at) waiters.splice(i, 1)[0].resolve();
    }
    requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);
  const wait = (ms: number) => new Promise<void>((resolve) => waiters.push({ at: clock + ms, resolve }));

  new IntersectionObserver(
    ([entry]) => {
      visible = entry.isIntersecting;
      visible ? dots.start() : dots.stop();
    },
    { threshold: 0.15 },
  ).observe(root);

  // Fake voice: syllable-ish bursts while a word is "spoken".
  let speaking = false;
  const voice = () => {
    if (speaking) {
      const t = performance.now() / 1000;
      dots.setLevel(0.45 + 0.35 * Math.abs(Math.sin(t * 9.1)) * (0.6 + 0.4 * Math.sin(t * 2.3)) + Math.random() * 0.12);
    } else {
      dots.setLevel(0.04);
    }
    requestAnimationFrame(voice);
  };
  requestAnimationFrame(voice);

  const setPhase = (phase: DotPhase | "hidden", ta = "", en = "") => {
    ui.overlay.dataset.phase = phase;
    ui.statusTa.textContent = ta;
    ui.statusEn.textContent = en;
    if (phase !== "hidden") dots.setPhase(phase);
  };

  const tap = async () => {
    for (let n = 0; n < 2; n++) {
      ui.key.classList.add("is-down");
      await wait(110);
      ui.key.classList.remove("is-down");
      await wait(130);
    }
  };

  const bubble = (who: string, text: string, lang: string, meaning?: string, mine = false) => {
    const p = document.createElement("p");
    p.className = mine ? "msg msg--mine" : "msg";
    const b = document.createElement("b");
    b.textContent = who;
    const s = document.createElement("span");
    s.lang = lang;
    s.textContent = text;
    p.append(b, s);
    if (meaning) {
      const m = document.createElement("small");
      m.textContent = meaning;
      p.append(m);
    }
    ui.thread.append(p);
  };

  const play = async (scene: Scene) => {
    ui.title.textContent = scene.title;
    ui.thread.replaceChildren();
    for (const [who, text, lang, meaning] of scene.thread) bubble(who, text, lang, meaning);
    ui.gloss.classList.remove("is-on");
    ui.glossText.textContent = scene.gloss ?? "";
    ui.composed.textContent = "";
    ui.icon.textContent = scene.icon;
    ui.icon.style.background = scene.iconBg;
    ui.app.textContent = scene.app;
    ui.field.textContent = scene.field;
    ui.mode.textContent = scene.mode;
    ui.text.lang = scene.lang;
    ui.text.textContent = "";
    ui.keylabel.textContent = "control × 2";
    setPhase("hidden");
    await wait(900);

    await tap();
    setPhase("listening", "கேட்கிறேன்…", "Listening");
    ui.gloss.classList.toggle("is-on", Boolean(scene.gloss));
    await wait(450);

    for (const word of scene.words) {
      speaking = true;
      await wait(140 + word.length * 26);
      ui.text.textContent = (ui.text.textContent ? ui.text.textContent + " " : "") + word;
      speaking = false;
      await wait(90 + Math.random() * 120);
    }
    await wait(500);

    await tap();
    setPhase("transcribing", "எழுதுகிறேன்…", "Writing it down");
    await wait(1250);

    ui.composed.textContent = ui.text.textContent;
    ui.composed.lang = scene.lang;
    ui.composer.classList.add("is-filled");
    setPhase("done", "சேர்த்தாச்சு ✓", `Inserted into ${scene.app}`);
    await wait(1500);

    setPhase("hidden");
    await wait(450);
    ui.composer.classList.remove("is-filled");
    ui.composed.textContent = "";
    ui.gloss.classList.remove("is-on");
    bubble("You", ui.text.textContent ?? "", scene.lang, scene.gloss, true);
    await wait(1800);
  };

  (async () => {
    for (let i = 0; ; i = (i + 1) % SCENES.length) await play(SCENES[i]);
  })();
}

// ── Story: the big ஃ cycles through its states ──
function runStoryDots(svg: SVGSVGElement) {
  const dots = new AythamDots(svg);
  const order: [DotPhase, number][] = [
    ["idle", 2200],
    ["listening", 2600],
    ["transcribing", 2600],
    ["done", 1400],
  ];
  let i = 0;
  let level = 0;
  const step = () => {
    const [phase, ms] = order[i];
    dots.setPhase(phase);
    i = (i + 1) % order.length;
    setTimeout(step, ms);
  };
  setInterval(() => {
    level = 0.3 + Math.random() * 0.6;
    dots.setLevel(level);
  }, 160);
  new IntersectionObserver(([e]) => (e.isIntersecting ? dots.start() : dots.stop())).observe(svg);
  step();
}

// ── Download buttons: put the visitor's OS first ──
function preferOs() {
  const ua = navigator.userAgent;
  const win = /Windows/i.test(ua);
  if (!win) return;
  const mac = document.querySelector<HTMLElement>('[data-os="mac"]');
  const w = document.querySelector<HTMLElement>('[data-os="win"]');
  if (!mac || !w) return;
  mac.classList.replace("btn--manjal", "btn--paper");
  w.classList.replace("btn--paper", "btn--manjal");
  w.after(mac);
}

// ── Mobile menu ──
function menu() {
  const btn = document.querySelector<HTMLButtonElement>(".menu-btn");
  const nav = document.getElementById("site-nav");
  if (!btn || !nav) return;
  const set = (open: boolean) => {
    btn.setAttribute("aria-expanded", String(open));
    nav.classList.toggle("is-open", open);
  };
  btn.addEventListener("click", () => set(btn.getAttribute("aria-expanded") !== "true"));
  nav.addEventListener("click", (e) => {
    if ((e.target as HTMLElement).closest("a")) set(false);
  });
  let openedAt = 0;
  btn.addEventListener("click", () => (openedAt = scrollY));
  window.addEventListener(
    "scroll",
    () => {
      if (nav.classList.contains("is-open") && Math.abs(scrollY - openedAt) > 60) set(false);
    },
    { passive: true },
  );
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && nav.classList.contains("is-open")) {
      set(false);
      btn.focus();
    }
  });
}

// ── Highlight the section in view ──
function activeSection() {
  const links = new Map<string, HTMLAnchorElement>();
  document.querySelectorAll<HTMLAnchorElement>('.nav a[href^="#"]').forEach((a) => links.set(a.hash.slice(1), a));
  const io = new IntersectionObserver(
    (entries) => {
      for (const e of entries) {
        if (!e.isIntersecting) continue;
        links.forEach((a) => a.removeAttribute("aria-current"));
        links.get(e.target.id)?.setAttribute("aria-current", "true");
      }
    },
    { rootMargin: "-45% 0px -50% 0px" },
  );
  links.forEach((_, id) => {
    const el = document.getElementById(id);
    if (el) io.observe(el);
  });
}

// ── Scroll reveal: content is visible without JS; this only adds the entrance ──
function reveal() {
  if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
  const groups = [".sec-head", ".steps", ".callout", ".repo-links", ".said", ".mode-cards", ".privacy__copy", ".receipt", ".poster", ".tales", ".temple", ".stamps", ".oss__grid", ".portrait", ".maker__copy", ".qa", ".kural"];
  const targets: HTMLElement[] = [];
  for (const sel of groups) {
    document.querySelectorAll<HTMLElement>(sel).forEach((el) => {
      // Lists reveal child by child; everything else as one block.
      const kids = el.matches(".steps, .mode-cards, .tales, .stamps") ? [...el.children] : [el];
      kids.forEach((k, i) => {
        const h = k as HTMLElement;
        h.classList.add("reveal");
        h.style.transitionDelay = `${Math.min(i, 5) * 80}ms`;
        targets.push(h);
      });
    });
  }
  const io = new IntersectionObserver(
    (entries) => {
      for (const e of entries) {
        if (e.isIntersecting) {
          e.target.classList.add("is-in");
          io.unobserve(e.target);
        }
      }
    },
    { rootMargin: "0px 0px -8% 0px", threshold: 0.08 },
  );
  targets.forEach((t) => io.observe(t));
}

menu();
activeSection();
reveal();

const demo = document.getElementById("demo");
if (demo) runDemo(demo);
const story = document.querySelector<SVGSVGElement>("[data-story-dots]");
if (story) runStoryDots(story);
preferOs();
