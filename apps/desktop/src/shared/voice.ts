// The voice indicator. One set of seven marks that morphs between states:
//   idle          the ஃ row: three dots, breathing
//   listening     a seven-bar waveform, tallest in the middle, driven by the mic level
//   transcribing  the bars fold back into the ஃ triangle, which spins inside a ring
//   done          the triangle settles upright and pulses (tinted green by CSS)
//   error         three dots shake (tinted kumkum by CSS)
// Every mark eases toward its target each frame, so any state can interrupt any other.
import "./voice.css";

export type VoiceState = "idle" | "listening" | "transcribing" | "done" | "error";

const N = 7;
const MID = 3;
const ENVELOPE = [0.36, 0.6, 0.84, 1, 0.84, 0.6, 0.36];
// Each bar wobbles at its own rate so the waveform never looks like a loop.
const FREQ = [5.1, 7.3, 6.2, 8.9, 6.7, 7.9, 5.6];
const PHASE = [0.2, 1.7, 3.1, 0.9, 2.4, 4.0, 5.2];

type Mark = { x: number; y: number; w: number; h: number; o: number };

export interface VoiceOptions {
  /** Height of the tallest bar, px. Everything else scales from it. */
  height?: number;
  /** Called every frame with the smoothed level (0–1), e.g. to drive a glow. */
  onLevel?: (level: number) => void;
}

const reduced = () => window.matchMedia("(prefers-reduced-motion: reduce)").matches;

export class Voice {
  readonly el: HTMLElement;
  private bars: HTMLElement[] = [];
  private cur: Mark[];
  private state: VoiceState = "idle";
  private since = performance.now();
  private target = 0;
  private level = 0;
  private spin = 0;
  private raf = 0;
  private last = performance.now();
  private alive = true;
  private readonly H: number;
  private readonly D: number;

  constructor(private opts: VoiceOptions = {}) {
    this.H = opts.height ?? 26;
    this.D = Math.round(this.H * 0.24);
    this.el = document.createElement("span");
    this.el.className = "voice-viz";
    this.el.dataset.state = "idle";
    this.el.style.setProperty("--vh", `${this.H}px`);
    const ring = document.createElement("i");
    ring.className = "voice-ring";
    this.el.append(ring);
    for (let i = 0; i < N; i++) {
      const b = document.createElement("i");
      b.className = "voice-bar";
      b.style.setProperty("--i", String(i));
      this.el.append(b);
      this.bars.push(b);
    }
    this.cur = this.targets(performance.now() / 1000, 0);
    this.paint();
    this.raf = requestAnimationFrame(this.frame);
  }

  setState(s: VoiceState) {
    if (s === this.state) return;
    if (s === "done") {
      const turn = (Math.PI * 2) / 3;
      this.spin = Math.round(this.spin / turn) * turn; // settle upright, not snap
    }
    this.state = s;
    this.since = performance.now();
    this.el.dataset.state = s;
  }

  setLevel(l: number) {
    this.target = Math.max(0, Math.min(1, l));
  }

  destroy() {
    this.alive = false;
    cancelAnimationFrame(this.raf);
  }

  private targets(t: number, dt: number): Mark[] {
    const { H, D } = this;
    const still = reduced();
    const el = (performance.now() - this.since) / 1000;
    const gap = D * 2.4; // ஃ row: gap 1.6× diameter
    const hidden = (i: number): Mark => ({ x: (i - MID) * gap * 0.5, y: 0, w: D * 0.7, h: D * 0.7, o: 0 });
    const out: Mark[] = [];

    switch (this.state) {
      case "idle": {
        const s = still ? 1 : 1 + 0.08 * (0.5 - 0.5 * Math.cos((t * Math.PI * 2) / 2.4));
        for (let i = 0; i < N; i++) out.push(Math.abs(i - MID) <= 1 ? { x: (i - MID) * gap, y: 0, w: D * s, h: D * s, o: 1 } : hidden(i));
        break;
      }
      case "error": {
        const p = Math.min(1, el / 0.52);
        const shake = still ? 0 : 5 * Math.sin(p * Math.PI * 6) * (1 - p);
        for (let i = 0; i < N; i++) out.push(Math.abs(i - MID) <= 1 ? { x: (i - MID) * gap + shake, y: 0, w: D, h: D, o: 1 } : hidden(i));
        break;
      }
      case "listening": {
        const w = Math.max(3, D * 0.78);
        const step = w * 1.9;
        const lv = this.level;
        for (let i = 0; i < N; i++) {
          const wob = still ? 0.75 : 0.55 + 0.45 * Math.sin(t * FREQ[i] + PHASE[i]);
          const amp = 0.14 + 0.86 * Math.min(1, lv * 1.25) * wob;
          out.push({ x: (i - MID) * step, y: 0, w, h: Math.max(w, H * ENVELOPE[i] * amp), o: 0.5 + 0.5 * ENVELOPE[i] });
        }
        break;
      }
      case "transcribing":
      case "done": {
        if (this.state === "transcribing" && !still) this.spin += dt * ((Math.PI * 2) / 1.2);
        const r = D * 1.25;
        const pulse = this.state === "done" && !still ? 1 + 0.28 * Math.sin(Math.PI * Math.min(1, el / 0.5)) : 1;
        const angles = [-Math.PI / 2, (5 * Math.PI) / 6, Math.PI / 6]; // top, bottom-left, bottom-right
        for (let i = 0; i < N; i++) {
          const k = i - (MID - 1);
          if (k < 0 || k > 2) {
            out.push({ x: 0, y: 0, w: D * 0.5, h: D * 0.5, o: 0 });
            continue;
          }
          const a = this.spin + angles[k];
          out.push({ x: Math.cos(a) * r, y: Math.sin(a) * r + D * 0.15, w: D * pulse, h: D * pulse, o: 1 });
        }
        break;
      }
    }
    return out;
  }

  private frame = (now: number) => {
    if (!this.alive) return;
    const dt = Math.min(0.05, (now - this.last) / 1000);
    this.last = now;
    // fast attack, slow release, like a VU meter
    const k = this.target > this.level ? 0.5 : 0.12;
    this.level += (this.target - this.level) * k;
    this.opts.onLevel?.(this.state === "listening" ? this.level : 0);

    const goal = this.targets(now / 1000, dt);
    const ease = reduced() ? 1 : 1 - Math.exp(-dt * 16);
    for (let i = 0; i < N; i++) {
      const c = this.cur[i];
      const g = goal[i];
      c.x += (g.x - c.x) * ease;
      c.y += (g.y - c.y) * ease;
      c.w += (g.w - c.w) * ease;
      c.h += (g.h - c.h) * ease;
      c.o += (g.o - c.o) * ease;
    }
    this.paint();
    this.raf = requestAnimationFrame(this.frame);
  };

  private paint() {
    for (let i = 0; i < N; i++) {
      const c = this.cur[i];
      const s = this.bars[i].style;
      s.width = `${c.w.toFixed(2)}px`;
      s.height = `${c.h.toFixed(2)}px`;
      s.opacity = c.o.toFixed(3);
      s.transform = `translate(${(c.x - c.w / 2).toFixed(2)}px, ${(c.y - c.h / 2).toFixed(2)}px)`;
    }
  }
}
