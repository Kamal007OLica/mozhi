// The ஃ dots — Mozhi's voice indicator, drawn into an <svg viewBox="-36 -24 72 48">.
// Motion spec (shared with the desktop overlay):
//   idle          row of three, gap 1.6× diameter, 2.4 s breathing (scale 1 → 1.08)
//   listening     travelling wave y = −A·sin(t·2π·1.6 − i·0.9), A from voice level (2 px … 1.1× d)
//   transcribing  glide 420 ms (cubic-bezier(.34,1.56,.64,1)) into the ஃ triangle, 1.2 s per turn, prabha ring
//   done          triangle settles upright and pulses once
//   error         3-cycle, 6 px horizontal shake, tinted kumkum

export type DotPhase = "idle" | "listening" | "transcribing" | "done" | "error";

const SVG_NS = "http://www.w3.org/2000/svg";
const D = 9; // dot diameter in viewBox units
const R = D / 2;
const SPACING = D * 2.6; // centre-to-centre: diameter + 1.6× gap
const TRI = D * 1.25; // triangle circumradius
const GLIDE_MS = 420;

const MANJAL = [244, 196, 48];
const BRIGHT = [255, 238, 176];
const KUMKUM = [229, 87, 79];

const reduceMotion = () => window.matchMedia("(prefers-reduced-motion: reduce)").matches;

function cubicBezier(x1: number, y1: number, x2: number, y2: number) {
  const cx = 3 * x1, bx = 3 * (x2 - x1) - cx, ax = 1 - cx - bx;
  const cy = 3 * y1, by = 3 * (y2 - y1) - cy, ay = 1 - cy - by;
  const sx = (t: number) => ((ax * t + bx) * t + cx) * t;
  const sy = (t: number) => ((ay * t + by) * t + cy) * t;
  const dx = (t: number) => (3 * ax * t + 2 * bx) * t + cx;
  return (x: number) => {
    let t = x;
    for (let i = 0; i < 6; i++) {
      const e = sx(t) - x;
      const d = dx(t);
      if (Math.abs(e) < 1e-4 || Math.abs(d) < 1e-6) break;
      t -= e / d;
    }
    return sy(Math.min(1, Math.max(0, t)));
  };
}

const spring = cubicBezier(0.34, 1.56, 0.64, 1);
const easeOut = cubicBezier(0.22, 1, 0.36, 1);
const mix = (a: number[], b: number[], k: number) => `rgb(${a.map((v, i) => Math.round(v + (b[i] - v) * k)).join(",")})`;

const row = (i: number) => ({ x: (i - 1) * SPACING, y: 0 });
// Triangle order: top, bottom-left, bottom-right — matches the logo.
const tri = (i: number, angle: number) => {
  const a = angle + [-Math.PI / 2, (5 * Math.PI) / 6, Math.PI / 6][i];
  return { x: Math.cos(a) * TRI, y: Math.sin(a) * TRI + D * 0.12 };
};

export class AythamDots {
  private dots: SVGCircleElement[] = [];
  private ring: SVGGElement;
  private phase: DotPhase = "idle";
  private since = performance.now();
  private from = [row(0), row(1), row(2)];
  private pos = [row(0), row(1), row(2)];
  private level = 0;
  private target = 0;
  private spin = 0;
  private raf = 0;
  private running = false;

  constructor(svg: SVGSVGElement) {
    svg.setAttribute("viewBox", "-36 -24 72 48");
    this.ring = document.createElementNS(SVG_NS, "g");
    this.ring.setAttribute("opacity", "0");
    const flames = 15;
    let d = "";
    for (let k = 0; k < flames; k++) {
      const a = (k / flames) * Math.PI * 2;
      const r0 = TRI + D * 0.95, r1 = r0 + 4.2;
      const p = (r: number, da: number) => `${(Math.cos(a + da) * r).toFixed(2)} ${(Math.sin(a + da) * r).toFixed(2)}`;
      d += `M${p(r0, -0.16)}Q${p(r0 + 2.4, -0.05)} ${p(r1, 0.1)}Q${p(r0 + 1.4, 0.12)} ${p(r0, 0.16)}Z`;
    }
    const circle = document.createElementNS(SVG_NS, "circle");
    circle.setAttribute("r", String(TRI + D * 0.95));
    circle.setAttribute("fill", "none");
    circle.setAttribute("stroke", "currentColor");
    circle.setAttribute("stroke-width", "1");
    const path = document.createElementNS(SVG_NS, "path");
    path.setAttribute("d", d);
    path.setAttribute("fill", "currentColor");
    this.ring.append(circle, path);
    this.ring.style.color = "rgb(244 196 48 / .55)";
    svg.append(this.ring);
    for (let i = 0; i < 3; i++) {
      const c = document.createElementNS(SVG_NS, "circle");
      c.setAttribute("r", String(R));
      c.setAttribute("fill", mix(MANJAL, MANJAL, 0));
      svg.append(c);
      this.dots.push(c);
    }
    this.draw(performance.now());
  }

  setPhase(phase: DotPhase) {
    if (phase === this.phase) return;
    this.from = this.pos.map((p) => ({ ...p }));
    if (phase === "done") {
      // Settle to the nearest upright orientation instead of snapping back.
      const turn = (Math.PI * 2) / 3;
      this.spin = Math.round(this.spin / turn) * turn;
    }
    this.phase = phase;
    this.since = performance.now();
    this.kick();
  }

  /** 0–1 voice level; smoothed internally (fast attack, slow release). */
  setLevel(level: number) {
    this.target = Math.max(0, Math.min(1, level));
  }

  start() {
    this.running = true;
    this.kick();
  }

  stop() {
    this.running = false;
    cancelAnimationFrame(this.raf);
    this.raf = 0;
  }

  private kick() {
    if (this.running && !this.raf) this.raf = requestAnimationFrame(this.frame);
  }

  private frame = (now: number) => {
    this.raf = 0;
    this.draw(now);
    if (this.running) this.raf = requestAnimationFrame(this.frame);
  };

  private draw(now: number) {
    const still = reduceMotion();
    const t = now / 1000;
    const el = now - this.since;
    const k = this.target > this.level ? 0.45 : 0.08;
    this.level += (this.target - this.level) * k;

    let scale = 1;
    let shake = 0;
    let ringOpacity = 0;
    const fills: string[] = [];
    const goal: { x: number; y: number }[] = [];

    switch (this.phase) {
      case "idle": {
        if (!still) scale = 1 + 0.04 * (1 - Math.cos((t * 2 * Math.PI) / 2.4));
        for (let i = 0; i < 3; i++) goal.push(row(i)), fills.push(mix(MANJAL, MANJAL, 0));
        break;
      }
      case "listening": {
        const A = still ? 0 : 2 + (D * 1.1 - 2) * this.level;
        for (let i = 0; i < 3; i++) {
          const s = Math.sin(t * 2 * Math.PI * 1.6 - i * 0.9);
          goal.push({ x: row(i).x, y: -A * s });
          fills.push(mix(MANJAL, BRIGHT, Math.max(0, s) * Math.min(1, this.level * 1.4)));
        }
        break;
      }
      case "transcribing": {
        if (!still) this.spin = ((t % 1.2) / 1.2) * Math.PI * 2;
        for (let i = 0; i < 3; i++) goal.push(tri(i, this.spin)), fills.push(mix(MANJAL, MANJAL, 0));
        ringOpacity = Math.min(1, el / GLIDE_MS);
        break;
      }
      case "done": {
        const p = Math.min(1, el / 520);
        if (!still) scale = 1 + 0.22 * Math.sin(Math.PI * p);
        for (let i = 0; i < 3; i++) goal.push(tri(i, this.spin)), fills.push(mix(MANJAL, BRIGHT, 1 - p));
        ringOpacity = Math.max(0, 1 - el / 300);
        break;
      }
      case "error": {
        const p = Math.min(1, el / 520);
        if (!still) shake = 6 * Math.sin(p * Math.PI * 2 * 3) * (1 - p);
        for (let i = 0; i < 3; i++) goal.push(row(i)), fills.push(mix(KUMKUM, KUMKUM, 0));
        break;
      }
    }

    const toTri = this.phase === "transcribing" || this.phase === "done";
    const g = still ? 1 : Math.min(1, el / GLIDE_MS);
    const e = toTri ? spring(g) : easeOut(g);
    for (let i = 0; i < 3; i++) {
      // In the wave phase only the glide's x is eased; y follows the wave directly once settled.
      const x = this.from[i].x + (goal[i].x - this.from[i].x) * e;
      const y = this.from[i].y + (goal[i].y - this.from[i].y) * e;
      this.pos[i] = { x, y };
      const c = this.dots[i];
      c.setAttribute("cx", (x + shake).toFixed(2));
      c.setAttribute("cy", y.toFixed(2));
      c.setAttribute("r", (R * scale).toFixed(2));
      c.setAttribute("fill", fills[i]);
    }
    this.ring.setAttribute("opacity", ringOpacity.toFixed(2));
    this.ring.setAttribute("transform", `rotate(${((t * 360) / 9) % 360})`);
  }
}
