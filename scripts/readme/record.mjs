#!/usr/bin/env node
// Records and renders every image in the README into .github/readme/.
//
//   npx vite --port 1430 --config apps/desktop/vite.config.ts apps/desktop     # the app UI, in a browser
//   node scripts/readme/record.mjs [task …]      tasks: banner hero voice app windows plates (default: all)
//   python3 scripts/readme/optimize.py            shrinks the PNGs and removes the raw app captures
//
// Needs Google Chrome and ffmpeg. Set UI=http://localhost:<port> if the desktop UI is not on 1430,
// and SITE=<url> to capture plates from somewhere other than the live site.
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { launchChrome, newPage } from "./cdp.mjs";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const OUT = join(ROOT, ".github/readme");
const UI = process.env.UI ?? "http://localhost:1430";
const SITE = process.env.SITE ?? "https://mozhi-khaki.vercel.app";
const stage = (name) => `${UI}/@fs${ROOT}/scripts/readme/stage/${name}`;
mkdirSync(OUT, { recursive: true });

const save = (name, buf) => {
  writeFileSync(join(OUT, name), buf);
  console.log(`  ${name}  ${(buf.length / 1024).toFixed(0)} KB`);
};
const sh = (cmd, args) => execFileSync(cmd, args, { stdio: ["ignore", "ignore", "inherit"] });

/** Frames from a screencast → MP4 (as recorded) and a looping GIF. */
function encode(frames, name, { gifWidth, fps = 24 }) {
  const dir = mkdtempSync(join(tmpdir(), `mozhi-${name}-`));
  const list = [];
  frames.forEach((f, i) => {
    const file = join(dir, `f${String(i).padStart(5, "0")}.png`);
    writeFileSync(file, f.data);
    const next = frames[i + 1]?.t ?? f.t + 1 / 30;
    list.push(`file '${file}'`, `duration ${(next - f.t).toFixed(4)}`);
  });
  list.push(`file '${join(dir, `f${String(frames.length - 1).padStart(5, "0")}.png`)}'`);
  writeFileSync(join(dir, "list.txt"), list.join("\n"));
  const input = ["-y", "-loglevel", "error", "-f", "concat", "-safe", "0", "-i", join(dir, "list.txt")];
  sh("ffmpeg", [...input, "-vf", "fps=30,format=yuv420p", "-c:v", "libx264", "-crf", "20", "-preset", "slow", "-movflags", "+faststart", join(OUT, `${name}.mp4`)]);
  sh("ffmpeg", [
    ...input,
    "-vf",
    `fps=${fps},scale=${gifWidth}:-1:flags=lanczos,split[a][b];[a]palettegen=max_colors=256:stats_mode=diff[p];[b][p]paletteuse=dither=sierra2_4a:diff_mode=rectangle`,
    "-loop",
    "0",
    join(OUT, `${name}.gif`),
  ]);
  rmSync(dir, { recursive: true, force: true });
  console.log(`  ${name}.mp4 + ${name}.gif  (${frames.length} frames, ${(frames.at(-1).t - frames[0].t).toFixed(1)} s)`);
}

const tasks = {
  async banner(page) {
    await page.viewport(1280, 560, 2);
    await page.transparent();
    await page.goto(stage("banner.html"));
    save("banner.png", await page.shot("#plate"));
  },

  async hero(page) {
    await page.viewport(1280, 720, 1.5);
    await page.goto(stage("hero.html"), 1500);
    await page.eval("window.nextStart()"); // let one full cycle play first
    const frames = await page.record(page.eval("window.nextStart()"));
    encode(frames, "demo", { gifWidth: 960, fps: 20 });
  },

  async voice(page) {
    await page.viewport(960, 360, 2);
    await page.goto(stage("voice.html"), 600);
    await page.eval("window.nextStart()");
    const frames = await page.record(page.eval("window.nextStart()"));
    encode(frames, "voice", { gifWidth: 720, fps: 24 });
  },

  async app(page) {
    await page.viewport(1080, 720, 2);
    await page.goto(`${UI}/?onboarded`, 1200);
    const click = (text) => page.eval(`(() => { const b = [...document.querySelectorAll("button")].find((b) => b.textContent.includes(${JSON.stringify(text)})); b?.click(); return !!b; })()`);
    await click("இப்போது வேண்டாம்"); // the one-time "open at login?" prompt
    for (const [name, label] of [["home", "Home"], ["models", "Models"], ["history", "History"]]) {
      if (!(await click(label))) throw new Error(`no nav button ${label}`);
      await new Promise((r) => setTimeout(r, 900));
      save(`app-${name}.raw.png`, await page.shot());
    }
  },

  async windows(page) {
    await page.viewport(1280, 760, 2);
    await page.transparent();
    await page.goto(stage("windows.html"));
    save("app.png", await page.shot("#plate"));
  },

  /** Sections of the live landing page, captured static (reduced motion, no sticky bar). */
  async plates(page) {
    await page.viewport(1280, 900, 2);
    await page.media([{ name: "prefers-reduced-motion", value: "reduce" }]);
    await page.goto(SITE, 1500);
    await page.eval(`(() => {
      const s = document.createElement("style");
      s.textContent = ".topbar{display:none!important} .reveal{opacity:1!important;translate:none!important} .foot__row,.foot__small,.repo-links{display:none!important}";
      document.head.append(s);
      document.querySelectorAll("img[loading=lazy]").forEach((i) => (i.loading = "eager"));
      return Promise.all([...document.images].map((i) => i.complete || new Promise((r) => (i.onload = i.onerror = r))));
    })()`);
    await new Promise((r) => setTimeout(r, 800));
    const plates = [
      ["plate-steps.png", "#how .steps", 34],
      ["plate-modes.png", "#modes .wrap", 0],
      ["plate-receipt.png", ".receipt", 40],
      ["plate-facts.png", ".facts", 0],
      ["plate-alphabet.png", ".kalvettu", 0],
      ["plate-poster.png", ".poster", 14],
      ["plate-tales.png", ".tales", 8],
      ["plate-temple.png", ".temple", 14],
      ["plate-stamps.png", ".stamps", 28],
      ["plate-maker.png", "#maker", 0],
      ["plate-kural.png", ".foot", 0],
    ];
    for (const [name, sel, pad] of plates) save(name, await page.shot(sel, pad));
  },
};

const wanted = process.argv.slice(2).length ? process.argv.slice(2) : Object.keys(tasks);
const { proc, port } = await launchChrome();
try {
  for (const name of wanted) {
    console.log(`▸ ${name}`);
    const page = await newPage(port);
    await tasks[name](page);
    page.close();
  }
} finally {
  proc.kill();
}
