// Scroll-smoothness benchmark. Run: node tests/perf/scroll.mjs [--out file.json] [--only name] [--throttle 4] [--software]
// Launches vite + headless Edge/Chrome (GPU on), drives the biggest scroll container of each page with a rAF
// triangle-wave scrollTop for 3 s, and reports frame time avg/p95, dropped frames (>20 ms) and trace costs (ms).
// Viewports: 1440x900@1 and 2560x1440@1.5. Set E2E_BROWSER to pick a browser.
import { spawn } from "node:child_process";
import { once } from "node:events";
import { existsSync, writeFileSync } from "node:fs";
import { mkdtemp, rm } from "node:fs/promises";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { Cdp, Page, poll } from "../e2e/harness.mjs";
import { placeholder, runes } from "../e2e/assets.mjs";

const root = fileURLToPath(new URL("../../", import.meta.url));
const arg = name => { const i = process.argv.indexOf(name); return i > 0 ? process.argv[i + 1] : null; };
const only = arg("--only"), outFile = arg("--out"), extraCss = arg("--css"), // --css "<rules>" injects CSS to A/B a fix
   throttle = Number(arg("--throttle") ?? 1); // --throttle 4 = 4x slower CPU
const VIEWPORTS = [{ width: 1440, height: 900, dpr: 1 }, { width: 2560, height: 1440, dpr: 1.5 }].filter((_, i) => arg("--vp") === null || i === Number(arg("--vp"))); // --vp 0|1
const SCROLL_MS = 3000, DROPPED_MS = 20;
const COSTS = ["UpdateLayoutTree", "Layout", "PrePaint", "Paint", "Layerize", "Commit", "RasterTask", "ImageDecodeTask", "FunctionCall", "EventDispatch"];
const fullApp = "/src/full-app/full-app-preview.html";

const navTo = label => async p => { await p.click('nav[aria-label="Swapper pages"] button', label); await p.text(".ui-page-header h1", label); };
const PAGES = [
  { name: "dashboard", url: fullApp, ready: ".match-row" },
  { name: "tierlist", url: fullApp, ready: ".match-row", steps: async p => { await navTo("Tierlist")(p); await p.visible(".tier-table tbody tr"); } },
  { name: "champion", url: fullApp, ready: ".match-row", steps: async p => {
    await navTo("Tierlist")(p); await p.click(".tier-champ", "Aatrox"); await p.visible(".fa-build-section"); } },
  { name: "match-stats", url: fullApp, ready: ".match-row", scroll: ".md-stats-scroll", steps: async p => {
    await p.click(".match-champ"); await p.visible(".md-page .md-team"); await p.click("#md-tab-Stats"); await p.visible(".md-stats-table"); } },
  { name: "accounts", url: fullApp, ready: ".match-row", steps: async p => { await navTo("Accounts")(p); await p.visible(".fa-account-card"); } },
  { name: "settings", url: fullApp, ready: ".match-row", steps: async p => { await navTo("Settings")(p); await p.visible(".fa-settings-content"); } },
  { name: "flyout", url: "/src/flyout-preview.html", ready: ".shell .main-panel" },
];

// Runs in the page: scrolls `selector` (or the largest scrollable element) and returns frame deltas.
function drive(selector, ms) {
  const scrollables = () => [...document.querySelectorAll("*")].filter(el => el.scrollHeight > el.clientHeight + 40 && /auto|scroll/.test(getComputedStyle(el).overflowY));
  const el = (selector && document.querySelector(selector)) || scrollables().sort((a, b) => b.scrollHeight - b.clientHeight - (a.scrollHeight - a.clientHeight))[0];
  if (!el) return Promise.resolve({ error: "no scroll container" });
  const range = el.scrollHeight - el.clientHeight, deltas = [];
  return new Promise(resolve => {
    let start, last;
    const tick = t => {
      start ??= t;
      if (last !== undefined) deltas.push(t - last);
      last = t;
      const phase = ((t - start) / 1500) % 2; // 1.5 s down, 1.5 s up
      el.scrollTop = range * (phase < 1 ? phase : 2 - phase);
      if (t - start < ms) requestAnimationFrame(tick);
      else resolve({ deltas, target: el.className || el.tagName, range, nodes: document.getElementsByTagName("*").length });
    };
    requestAnimationFrame(tick);
  });
}

const pct = (a, q) => [...a].sort((x, y) => x - y)[Math.min(a.length - 1, Math.floor(a.length * q))];
// Per-frame cost from the trace: main thread (BeginMainFrame) and GPU process (GPUTask), ms.
function perFrame(events) {
  const main = events.filter(e => e.ph === "X" && e.name === "ProxyMain::BeginMainFrame").map(e => e.dur / 1000);
  const gpu = events.filter(e => e.ph === "X" && e.name === "GPUTask").reduce((a, e) => a + e.dur / 1000, 0);
  const avg = a => a.reduce((x, y) => x + y, 0) / Math.max(1, a.length);
  return { mainAvg: +avg(main).toFixed(2), mainP95: +(pct(main, 0.95) ?? 0).toFixed(2), gpuAvg: +(gpu / Math.max(1, main.length)).toFixed(2) };
}
function summarize(events) {
  const sums = Object.fromEntries(COSTS.map(n => [n, 0]));
  for (const e of events) if (e.ph === "X" && e.name in sums) sums[e.name] += (e.dur ?? 0) / 1000;
  return Object.fromEntries(Object.entries(sums).map(([k, v]) => [k, Math.round(v)]));
}

async function freePort() {
  const s = createServer(); s.listen(0, "127.0.0.1"); await once(s, "listening");
  const port = s.address().port; await new Promise(r => s.close(r)); return port;
}
const children = [];
const launch = (cmd, args) => { const c = spawn(cmd, args, { cwd: root, windowsHide: true, stdio: ["ignore", "pipe", "pipe"] }); c.output = ""; for (const s of [c.stdout, c.stderr]) s.on("data", d => { c.output = (c.output + d).slice(-2000); }); children.push(c); return c; };
async function stop(c) { // our PIDs only, never an image-name kill
  if (!c.pid || c.exitCode !== null) return;
  if (process.platform === "win32") { const k = spawn("taskkill.exe", ["/PID", String(c.pid), "/T", "/F"], { windowsHide: true, stdio: "ignore" }); await once(k, "exit"); } else c.kill("SIGTERM");
}

async function main() {
  const exe = [process.env.E2E_BROWSER, process.env["PROGRAMFILES(X86)"] && path.join(process.env["PROGRAMFILES(X86)"], "Microsoft/Edge/Application/msedge.exe"),
    process.env.PROGRAMFILES && path.join(process.env.PROGRAMFILES, "Google/Chrome/Application/chrome.exe")].filter(Boolean).find(existsSync);
  if (!exe) throw new Error("Edge/Chrome not found. Set E2E_BROWSER.");
  const profile = await mkdtemp(path.join(tmpdir(), "swapper-perf-profile-"));
  const vitePort = await freePort(), debugPort = await freePort(), origin = `http://127.0.0.1:${vitePort}`;
  launch(process.execPath, ["node_modules/vite/bin/vite.js", "--host", "127.0.0.1", "--port", String(vitePort), "--strictPort"]);
  await poll("vite", async () => (await fetch(origin + fullApp)).ok, 15000, 200);
  launch(exe, ["--headless=new", `--user-data-dir=${profile}`, "--no-first-run", "--disable-extensions", "--disable-background-networking",
    ...(process.argv.includes("--software") ? ["--disable-gpu"] : []), // the shipped WebView2 runs with --disable-gpu
    "--disable-renderer-backgrounding", "--disable-background-timer-throttling", "--disable-backgrounding-occluded-windows",
    `--remote-debugging-port=${debugPort}`, "about:blank"]);
  const endpoint = `http://127.0.0.1:${debugPort}`;
  const version = await poll("browser", async () => (await fetch(endpoint + "/json/version")).json(), 10000, 100);
  const browser = await Cdp.connect(version.webSocketDebuggerUrl);
  const { targetId } = await browser.send("Target.createTarget", { url: "about:blank" });
  const target = (await (await fetch(endpoint + "/json/list")).json()).find(t => t.id === targetId);
  const cdp = await Cdp.connect(target.webSocketDebuggerUrl), page = new Page(cdp);
  cdp.on("Fetch.requestPaused", ({ requestId, request }) => {
    const url = new URL(request.url);
    if (url.origin === origin) return void cdp.send("Fetch.continueRequest", { requestId }).catch(() => {});
    const json = url.pathname.endsWith("/runesReforged.json");
    void cdp.send("Fetch.fulfillRequest", { requestId, responseCode: 200, responseHeaders: [{ name: "Content-Type", value: json ? "application/json" : "image/svg+xml" }, { name: "Access-Control-Allow-Origin", value: "*" }],
      body: Buffer.from(json ? JSON.stringify(runes) : placeholder).toString("base64") }).catch(() => {}); // stale after navigation
  });
  for (const d of ["Page", "Runtime", "Network"]) await cdp.send(`${d}.enable`);
  await cdp.send("Fetch.enable", { patterns: [{ urlPattern: "http*" }] });

  const results = [];
  for (const vp of VIEWPORTS) for (const spec of PAGES.filter(p => !only || p.name === only)) {
    await cdp.send("Emulation.setDeviceMetricsOverride", { width: vp.width, height: vp.height, deviceScaleFactor: vp.dpr, mobile: false });
    await cdp.send("Emulation.setCPUThrottlingRate", { rate: 1 }); // throttle only while scrolling
    await cdp.send("Page.navigate", { url: origin + spec.url });
    await page.visible(spec.ready);
    await spec.steps?.(page);
    if (extraCss) await page.evaluate(`document.head.append(Object.assign(document.createElement("style"), { textContent: ${JSON.stringify(extraCss)} }))`);
    await page.settle();
    await cdp.send("Emulation.setCPUThrottlingRate", { rate: throttle });
    const events = [];
    cdp.on("Tracing.dataCollected", ({ value }) => events.push(...value));
    await cdp.send("Tracing.start", { traceConfig: { includedCategories: ["devtools.timeline", "disabled-by-default-devtools.timeline", "cc", "gpu"] } }, 10000);
    const run = await cdp.send("Runtime.evaluate", { expression: `(${drive.toString()})(${JSON.stringify(spec.scroll ?? null)}, ${SCROLL_MS})`, returnByValue: true, awaitPromise: true }, 20000);
    await new Promise(resolve => { cdp.on("Tracing.tracingComplete", resolve); void cdp.send("Tracing.end", {}, 10000); });
    const r = run.result.value;
    if (r.error) { results.push({ page: spec.name, viewport: `${vp.width}x${vp.height}@${vp.dpr}`, error: r.error }); continue; }
    const d = r.deltas.slice(2); // drop warm-up frames
    const row = { page: spec.name, viewport: `${vp.width}x${vp.height}@${vp.dpr}`, container: r.target, range: r.range, nodes: r.nodes, frames: d.length,
      avgMs: +(d.reduce((a, b) => a + b, 0) / d.length).toFixed(1), p95Ms: +pct(d, 0.95).toFixed(1), dropped: d.filter(x => x > DROPPED_MS).length, perFrameMs: perFrame(events), costsMs: summarize(events) };
    results.push(row);
    console.log(`${row.viewport.padEnd(14)} ${row.page.padEnd(12)} avg ${String(row.avgMs).padStart(5)}  p95 ${String(row.p95Ms).padStart(5)}  dropped ${row.dropped}/${row.frames}  frame ${JSON.stringify(row.perFrameMs)}  ${JSON.stringify(row.costsMs)}`);
  }
  if (outFile) writeFileSync(outFile, JSON.stringify(results, null, 2));
  browser.close(); cdp.close();
  await rm(profile, { recursive: true, force: true, maxRetries: 8, retryDelay: 100 }).catch(() => {});
}
try { await main(); } catch (e) { console.error(e.stack); process.exitCode = 1; } finally { for (const c of children.toReversed()) await stop(c); }
