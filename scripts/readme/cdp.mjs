// A tiny Chrome DevTools Protocol client: no dependencies, Node 22's built-in WebSocket.
// Used by record.mjs to screenshot and screen-record the README media.
import { spawn } from "node:child_process";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

export async function launchChrome({ port = 9333 } = {}) {
  const chrome = process.env.CHROME ?? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
  const profile = mkdtempSync(join(tmpdir(), "mozhi-readme-"));
  const proc = spawn(
    chrome,
    [
      "--headless=new",
      `--remote-debugging-port=${port}`,
      `--user-data-dir=${profile}`,
      "--hide-scrollbars",
      "--force-color-profile=srgb",
      "--font-render-hinting=none",
      "--mute-audio",
      "about:blank",
    ],
    { stdio: "ignore" },
  );
  for (let i = 0; i < 100; i++) {
    try {
      if ((await fetch(`http://127.0.0.1:${port}/json/version`)).ok) return { proc, port };
    } catch {}
    await sleep(100);
  }
  proc.kill();
  throw new Error("Chrome did not start");
}

export async function newPage(port) {
  const target = await (await fetch(`http://127.0.0.1:${port}/json/new?about:blank`, { method: "PUT" })).json();
  const ws = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    ws.onopen = resolve;
    ws.onerror = reject;
  });
  let id = 0;
  const pending = new Map();
  const listeners = new Map();
  ws.onmessage = (msg) => {
    const data = JSON.parse(msg.data);
    if (data.id && pending.has(data.id)) {
      const { resolve, reject } = pending.get(data.id);
      pending.delete(data.id);
      data.error ? reject(new Error(`${data.error.message}`)) : resolve(data.result);
    } else if (data.method) {
      for (const fn of listeners.get(data.method) ?? []) fn(data.params);
    }
  };
  const send = (method, params = {}) =>
    new Promise((resolve, reject) => {
      const i = ++id;
      pending.set(i, { resolve, reject });
      ws.send(JSON.stringify({ id: i, method, params }));
    });
  const on = (method, fn) => listeners.set(method, [...(listeners.get(method) ?? []), fn]);
  const once = (method) => new Promise((resolve) => on(method, resolve));

  await send("Page.enable");
  await send("Runtime.enable");

  const page = {
    send,
    on,
    async viewport(width, height, scale = 2) {
      await send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: scale, mobile: false });
    },
    async transparent() {
      await send("Emulation.setDefaultBackgroundColorOverride", { color: { r: 0, g: 0, b: 0, a: 0 } });
    },
    async media(features) {
      await send("Emulation.setEmulatedMedia", { features });
    },
    async goto(url, settle = 800) {
      const loaded = once("Page.loadEventFired");
      await send("Page.navigate", { url });
      await loaded;
      await page.eval("document.fonts.ready.then(() => true)");
      await sleep(settle);
    },
    async eval(expression) {
      const r = await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
      if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description ?? r.exceptionDetails.text);
      return r.result.value;
    },
    /** PNG of a CSS selector's box (or the whole viewport). Returns a Buffer. */
    async shot(selector, pad = 0) {
      let clip;
      if (selector) {
        const box = await page.eval(`(() => { const r = document.querySelector(${JSON.stringify(selector)}).getBoundingClientRect(); return { x: r.left + scrollX, y: r.top + scrollY, width: r.width, height: r.height }; })()`);
        clip = { x: box.x - pad, y: box.y - pad, width: box.width + 2 * pad, height: box.height + 2 * pad, scale: 1 };
      }
      const r = await send("Page.captureScreenshot", { format: "png", captureBeyondViewport: true, ...(clip ? { clip } : {}) });
      return Buffer.from(r.data, "base64");
    },
    /** Screen-record until `until` resolves. Returns [{ data: Buffer, t: seconds }]. */
    async record(until) {
      const frames = [];
      on("Page.screencastFrame", (p) => {
        frames.push({ data: Buffer.from(p.data, "base64"), t: p.metadata.timestamp });
        send("Page.screencastFrameAck", { sessionId: p.sessionId }).catch(() => {});
      });
      await send("Page.startScreencast", { format: "png", everyNthFrame: 1 });
      await until;
      await send("Page.stopScreencast");
      return frames;
    },
    close: () => ws.close(),
  };
  return page;
}
