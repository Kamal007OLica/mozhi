// The three dots — Mozhi's voice. One renderer drives every state:
//
//   idle          · · ·      row, slow breathing
//   listening     ·˙·.·      travelling wave, amplitude follows the voice
//   transcribing    ஃ        dots glide into the āytham triangle, which turns inside a ring of fire
//   done            ஃ        triangle settles upright and pulses once
//   error         · · ·      shake, tinted kumkum
//
// Geometry and timings match the brand book and landing page (docs/ARCHITECTURE.md → motion).

export type DotsState = "idle" | "listening" | "transcribing" | "done" | "error";

interface Options {
  /** Dot diameter in CSS px. */
  size?: number;
  color?: string;
  errorColor?: string;
  /** Show the prabha (ring of fire) while transcribing. */
  ring?: boolean;
}

const SVG_NS = "http://www.w3.org/2000/svg";
const reduceMotion = () => window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;

export class Dots {
  readonly el: SVGSVGElement;
  private circles: SVGCircleElement[] = [];
  private ring: SVGGElement | null = null;
  private state: DotsState = "idle";
  private level = 0;
  private smooth = 0;
  private since = performance.now();
  private pos = [
    { x: 0, y: 0 },
    { x: 0, y: 0 },
    { x: 0, y: 0 },
  ];
  private raf = 0;
  private readonly d: number;
  private readonly w: number;
  private readonly h: number;
  private readonly color: string;
  private readonly errorColor: string;

  constructor(opts: Options = {}) {
    this.d = opts.size ?? 9;
    this.color = opts.color ?? "var(--mz-manjal)";
    this.errorColor = opts.errorColor ?? "var(--mz-kumkum-300)";
    const gap = this.d * 1.6;
    this.w = Math.round(this.d * 3 + gap * 2 + this.d * 2);
    this.h = Math.round(this.d * 4.2);

    this.el = document.createElementNS(SVG_NS, "svg");
    this.el.setAttribute("viewBox", `0 0 ${this.w} ${this.h}`);
    this.el.setAttribute("width", String(this.w));
    this.el.setAttribute("height", String(this.h));
    this.el.setAttribute("aria-hidden", "true");
    this.el.classList.add("mz-dots");
    this.el.style.overflow = "visible";

    if (opts.ring !== false) {
      this.ring = document.createElementNS(SVG_NS, "g");
      this.ring.innerHTML = prabhaPath(this.w / 2, this.h / 2, this.d * 1.55, this.d * 1.95, 15);
      this.ring.style.opacity = "0";
      this.ring.style.transition = "opacity 300ms ease";
      this.ring.setAttribute("fill", this.color);
      this.ring.style.color = this.color;
      this.el.appendChild(this.ring);
    }
    for (let i = 0; i < 3; i++) {
      const c = document.createElementNS(SVG_NS, "circle");
      c.setAttribute("r", String(this.d / 2));
      c.style.fill = this.color;
      c.style.transition = "fill 200ms ease";
      this.el.appendChild(c);
      this.circles.push(c);
    }
    const row = this.rowTargets(0);
    row.forEach((p, i) => (this.pos[i] = { ...p }));
    this.paint(row.map(() => 1));
    this.loop = this.loop.bind(this);
    this.raf = requestAnimationFrame(this.loop);
  }

  setState(state: DotsState) {
    if (state === this.state) return;
    this.state = state;
    this.since = performance.now();
    const err = state === "error";
    this.circles.forEach((c) => (c.style.fill = err ? this.errorColor : this.color));
    if (this.ring) this.ring.style.opacity = state === "transcribing" ? "0.3" : "0";
  }

  /** 0–1 perceptual level from the engine. */
  setLevel(level: number) {
    this.level = Math.max(0, Math.min(1, level));
  }

  destroy() {
    cancelAnimationFrame(this.raf);
  }

  // ── animation ──────────────────────────────────────────────────────────

  private rowTargets(t: number) {
    const cx = this.w / 2;
    const cy = this.h / 2;
    const gap = this.d * 2.6;
    return [-1, 0, 1].map((k, i) => {
      let y = cy;
      if (this.state === "listening") {
        const amp = 2 + this.smooth * this.d * 1.1;
        y = cy - amp * Math.sin(t * 2 * Math.PI * 1.6 - i * 0.9);
      }
      return { x: cx + k * gap, y };
    });
  }

  private triangleTargets(angle: number) {
    const cx = this.w / 2;
    const cy = this.h / 2 + this.d * 0.1;
    const r = this.d * 1.15;
    // top, bottom-left, bottom-right — ஃ
    return [-90, 150, 30].map((deg) => {
      const a = ((deg * Math.PI) / 180) + angle;
      return { x: cx + r * Math.cos(a), y: cy + r * Math.sin(a) };
    });
  }

  private loop(now: number) {
    this.raf = requestAnimationFrame(this.loop);
    const t = now / 1000;
    const age = (now - this.since) / 1000;
    const still = reduceMotion();

    // fast attack, slow release — the wave should jump with a syllable and settle gently
    const k = this.level > this.smooth ? 0.45 : 0.08;
    this.smooth += (this.level - this.smooth) * k;

    let targets: { x: number; y: number }[];
    let scales = [1, 1, 1];
    switch (this.state) {
      case "transcribing":
        targets = this.triangleTargets(still ? 0 : (age * 2 * Math.PI) / 1.2);
        if (this.ring && !still) this.ring.setAttribute("transform", `rotate(${(age * 40) % 360} ${this.w / 2} ${this.h / 2})`);
        break;
      case "done": {
        targets = this.triangleTargets(0);
        const p = Math.min(1, age / 0.36);
        const pulse = still ? 1 : 1 + Math.sin(p * Math.PI) * 0.28;
        scales = [pulse, pulse, pulse];
        break;
      }
      case "error": {
        targets = this.rowTargets(t);
        const shake = still || age > 0.42 ? 0 : Math.sin(age * 2 * Math.PI * 7) * 6 * (1 - age / 0.42);
        targets = targets.map((p) => ({ x: p.x + shake, y: p.y }));
        break;
      }
      case "listening":
        targets = still ? this.rowTargets(0) : this.rowTargets(t);
        scales = targets.map((p) => 1 + Math.max(0, (this.h / 2 - p.y) / (this.d * 6)));
        break;
      default: {
        targets = this.rowTargets(t);
        const b = still ? 1 : 1 + 0.04 * (1 + Math.sin((t * 2 * Math.PI) / 2.4));
        scales = [b, b, b];
      }
    }

    // critically-damped-ish follow; the glide into ஃ is the 420 ms spring from the motion spec
    const follow = this.state === "listening" ? 0.5 : 0.16;
    for (let i = 0; i < 3; i++) {
      this.pos[i].x += (targets[i].x - this.pos[i].x) * (still ? 1 : follow);
      this.pos[i].y += (targets[i].y - this.pos[i].y) * (still ? 1 : follow);
    }
    this.paint(scales);
  }

  private paint(scales: number[]) {
    for (let i = 0; i < 3; i++) {
      const c = this.circles[i];
      c.setAttribute("cx", this.pos[i].x.toFixed(2));
      c.setAttribute("cy", this.pos[i].y.toFixed(2));
      c.setAttribute("r", ((this.d / 2) * scales[i]).toFixed(2));
    }
  }
}

/** Nataraja's prabha: a ring with flame tongues. Same construction as brand/scripts/generate.py. */
export function prabhaPath(cx: number, cy: number, r0: number, r1: number, flames: number) {
  let d = "";
  for (let k = 0; k < flames; k++) {
    const a = (2 * Math.PI * k) / flames;
    const a1 = a - (Math.PI / flames) * 0.8;
    const a2 = a + (Math.PI / flames) * 0.8;
    const p = (r: number, ang: number) => `${(cx + r * Math.cos(ang)).toFixed(1)} ${(cy + r * Math.sin(ang)).toFixed(1)}`;
    const rm = r0 + (r1 - r0) * 0.55;
    d += `M${p(r0, a1)} Q${p(rm, a - 0.08)} ${p(r1, a + 0.12)} Q${p((r0 + rm) / 2, (a2 + a - 0.08) / 2)} ${p(r0, a2)}Z`;
  }
  return `<circle cx="${cx}" cy="${cy}" r="${r0}" fill="none" stroke="currentColor" stroke-width="1"/><path d="${d}"/>`;
}

/** Static ஃ mark as inline SVG (for logos inside the UI). */
export function aythamMark(size = 28, bg = "var(--mz-kumkum)", dot = "var(--mz-manjal)") {
  const r = size * 0.092;
  const dx = size * 0.158;
  const up = size * 0.158;
  const down = size * 0.112;
  const c = size / 2;
  return `<svg viewBox="0 0 ${size} ${size}" width="${size}" height="${size}" aria-hidden="true"><rect width="${size}" height="${size}" rx="${size * 0.225}" fill="${bg}"/><circle cx="${c}" cy="${c - up}" r="${r}" fill="${dot}"/><circle cx="${c - dx}" cy="${c + down}" r="${r}" fill="${dot}"/><circle cx="${c + dx}" cy="${c + down}" r="${r}" fill="${dot}"/></svg>`;
}
