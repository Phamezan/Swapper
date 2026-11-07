import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { existsSync } from "node:fs";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { allowance, Cdp, horizontalOverflow, Page, poll } from "./harness.mjs";
import { placeholder, runes } from "./assets.mjs";

const root = fileURLToPath(new URL("../../", import.meta.url));
const started = performance.now();
const capture = process.argv.includes("--screenshots");
const matchArg = process.argv.slice(2).find(arg => arg.startsWith("--match="));
const match = matchArg ? new RegExp(matchArg.slice("--match=".length)) : null;
const unknown = process.argv.slice(2).filter(arg => arg !== "--screenshots" && arg !== matchArg);
assert.equal(unknown.length, 0, `Unknown arguments: ${unknown.join(" ")}`);
assert.ok(Number(process.versions.node.split(".")[0]) >= 22 && typeof WebSocket === "function", "Use Node 22+ with built-in WebSocket");
const processes = [];
const connections = [];
const results = [];
const issues = [];
const allowed = new Map();
let issueIndex = 0;
let profile, screenshots, origin, browser, page, cleanupPromise;
let current = "startup";

function launch(command, args) {
  const child = spawn(command, args, { cwd: root, windowsHide: true, stdio: ["ignore", "pipe", "pipe"] });
  child.output = "";
  for (const stream of [child.stdout, child.stderr]) stream.on("data", value => { child.output = (child.output + value).slice(-5000); });
  child.on("error", error => { child.launchError = error; });
  processes.push(child);
  return child;
}

async function freePort() {
  const server = createServer();
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  const port = server.address().port;
  await new Promise((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
  return port;
}

async function getJson(url, init) {
  const response = await fetch(url, { ...init, signal: AbortSignal.timeout(700) });
  assert.ok(response.ok, `${url}: HTTP ${response.status}`);
  return response.json();
}

async function stop(child) {
  if (!child.pid || child.exitCode !== null || child.signalCode !== null) return;
  if (process.platform === "win32") {
    // Only our recorded PID and its children; never use an image-name kill.
    const killer = spawn("taskkill.exe", ["/PID", String(child.pid), "/T", "/F"], { windowsHide: true, stdio: "ignore" });
    await once(killer, "exit");
  } else {
    child.kill("SIGTERM");
    await poll(`PID ${child.pid} exit`, () => child.exitCode !== null || child.signalCode !== null, 1500)
      .catch(() => { child.kill("SIGKILL"); });
  }
}

function cleanup() {
  return cleanupPromise ??= (async () => {
    for (const connection of connections) connection.close();
    for (const child of processes.toReversed()) await stop(child);
    if (profile) {
      assert.equal(path.dirname(profile), path.resolve(tmpdir()));
      assert.ok(path.basename(profile).startsWith("swapper-e2e-profile-"));
      await rm(profile, { recursive: true, force: true, maxRetries: 8, retryDelay: 100 });
    }
  })();
}

for (const signal of ["SIGINT", "SIGTERM"]) process.once(signal, () => {
  void cleanup().finally(() => process.exit(signal === "SIGINT" ? 130 : 143));
});
const watchdog = setTimeout(() => {
  console.error(`FAIL suite exceeded 180 seconds during ${current}`);
  void cleanup().finally(() => process.exit(1));
}, 180_000);

async function audit() {
  await page.settle();
  const errors = [];
  for (const issue of issues.slice(issueIndex)) {
    const reason = allowance(issue);
    if (reason) allowed.set(reason, (allowed.get(reason) ?? 0) + 1);
    else errors.push(`${issue.kind} (${issue.page}): ${issue.text ?? `${issue.url} HTTP ${issue.status}`}`);
  }
  issueIndex = issues.length;
  errors.push(...await page.evaluate(`(${horizontalOverflow.toString()})()`));
  assert.ok(errors.length === 0, errors.join("\n"));
}

async function screenshot(name, width = 1280, height = 820) {
  if (!capture) return;
  await page.cdp.send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: 1, mobile: false });
  await page.settle();
  const { data } = await page.cdp.send("Page.captureScreenshot", { format: "png", captureBeyondViewport: false });
  const filename = `${name.replace(/[^a-z0-9]+/gi, "-").toLowerCase()}-${width}x${height}.png`;
  await writeFile(path.join(screenshots, filename), Buffer.from(data, "base64"));
}

async function check(name, work) {
  if (match && !match.test(name)) return;
  current = name;
  const start = performance.now();
  let failure;
  try { await work(); } catch (error) { failure = error.message; }
  try { await audit(); } catch (error) { failure = [failure, error.message].filter(Boolean).join("\n"); }
  try { await screenshot(name); } catch (error) { failure = [failure, `Screenshot: ${error.message}`].filter(Boolean).join("\n"); }
  results.push({ name, passed: !failure, ms: Math.round(performance.now() - start), ...(failure && { failure }) });
  console.log(`${failure ? "FAIL" : "PASS"} ${name} (${results.at(-1).ms}ms)`);
  if (failure) console.error(`  ${failure.replaceAll("\n", "\n  ")}`);
}

const fullApp = "/src/full-app/full-app-preview.html";
const draft = "/src/full-app/champ-select-preview.html";
const setup = "/src/onboarding/onboarding-preview.html";

async function navigate(url, selector) {
  const result = await page.cdp.send("Page.navigate", { url: origin + url });
  assert.ok(!result.errorText, result.errorText);
  // A cold Vite start can take longer than one poll window on the first page.
  await page.visible(selector).catch(() => page.visible(selector));
}

async function shell() {
  await page.visible('nav[aria-label="Swapper pages"]');
  for (const label of ["Dashboard", "Tierlist", "Matchups", "Draft", "Accounts", "Settings"]) {
    await page.text('nav[aria-label="Swapper pages"] button', label);
  }
  assert.equal(await page.evaluate("[...document.querySelectorAll('nav[aria-label=\"Swapper pages\"] button')].filter(b => b.textContent.includes('Live game')).length"), 0);
  if (!await page.evaluate("document.querySelector('nav[aria-label=\"Swapper pages\"] [aria-current=\"page\"]')?.textContent.includes('Draft')")) await page.visible(".ui-page-header h1");
}

async function nav(label, selector) {
  await page.click(`nav[aria-label="Swapper pages"] button[aria-label="${label}"]`);
  await page.text('nav[aria-label="Swapper pages"] [aria-current="page"]', label);
  if (label !== "Draft") await page.text(".ui-page-header h1", label);
  await page.visible(selector);
}

async function championGridLayout() {
  const measure = "(() => { const el = document.querySelector('.fa-champion-grid'); return {columns: getComputedStyle(el).gridTemplateColumns.split(' ').length, width: el.clientWidth, available: el.parentElement.clientWidth}; })()";
  for (const width of [1280, 1650, 1920]) {
    await page.cdp.send("Emulation.setDeviceMetricsOverride", { width, height: 900, deviceScaleFactor: 1, mobile: false });
    for (const centered of [false, true]) {
      // Also exercise the centered parent that caused the real-app collapse.
      if (centered) await page.evaluate("Object.assign(document.querySelector('.fa-matchup-empty').style, {display: 'grid', justifyItems: 'center'})");
      try {
        const grid = await page.evaluate(measure);
        assert.ok(grid.columns > 1, `${width}px: expected multiple champion columns, got ${grid.columns}`);
        assert.ok(grid.width >= grid.available - 1, `${width}px: champion grid must fill its container`);
        if (!centered) {
          const hero = await page.evaluate("(() => { const parent = document.querySelector('.fa-matchup-empty'); const box = parent.getBoundingClientRect(); const yours = parent.querySelector('.cs-slot.is-you'); const enemy = parent.querySelector('.cs-slot.is-empty'); const title = parent.querySelector('.fa-matchup-prompt h2'); const center = yours ? (yours.getBoundingClientRect().left + enemy.getBoundingClientRect().right) / 2 : title.getBoundingClientRect().left + title.getBoundingClientRect().width / 2; const context = parent.querySelector('.fa-matchup-context')?.getBoundingClientRect(); return {offset: Math.abs(center - box.left - box.width / 2), contextOffset: context ? Math.abs(context.left + context.width / 2 - box.left - box.width / 2) : 0, titleCentered: !title || getComputedStyle(title).textAlign === 'center', hasSteps: /Step [12] of 2/.test(parent.textContent)}; })()");
          assert.ok(hero.offset <= 1 && hero.contextOffset <= 1 && hero.titleCentered, `${width}px: matchup hero and context must be centered`);
          assert.equal(hero.hasSteps, false, "hero must not show step labels");
        }
      } finally { await page.evaluate("document.querySelector('.fa-matchup-empty').removeAttribute('style')"); }
    }
  }
  await page.cdp.send("Emulation.setDeviceMetricsOverride", { width: 1280, height: 820, deviceScaleFactor: 1, mobile: false });
}

// Every search box: the rounded box shows focus, never a square on the bare input.
async function searchPillFocus(box = '.search-pill') {
  await page.evaluate(`document.querySelector('${box} input').focus()`);
  const focus = await page.evaluate(`(() => { const input = document.querySelector('${box} input'); const pill = getComputedStyle(input.closest('label')); return {inputOutline: getComputedStyle(input).outlineStyle, pillOutline: pill.outlineStyle, pillOutlineWidth: parseFloat(pill.outlineWidth), radius: parseFloat(pill.borderRadius)}; })()`);
  assert.equal(focus.inputOutline, "none", "search input must not draw a square focus outline");
  assert.equal(focus.pillOutline, "solid", "search pill must show keyboard focus");
  assert.ok(focus.pillOutlineWidth >= 2 && focus.radius > 0, "focus ring belongs to the rounded pill");
}

async function main() {
  const candidates = [
    process.env.E2E_BROWSER,
    process.env["PROGRAMFILES(X86)"] && path.join(process.env["PROGRAMFILES(X86)"], "Microsoft/Edge/Application/msedge.exe"),
    process.env.PROGRAMFILES && path.join(process.env.PROGRAMFILES, "Microsoft/Edge/Application/msedge.exe"),
    process.env.PROGRAMFILES && path.join(process.env.PROGRAMFILES, "Google/Chrome/Application/chrome.exe"),
    process.env.LOCALAPPDATA && path.join(process.env.LOCALAPPDATA, "Microsoft/Edge/Application/msedge.exe"),
    process.env.LOCALAPPDATA && path.join(process.env.LOCALAPPDATA, "Google/Chrome/Application/chrome.exe"),
    "/usr/bin/microsoft-edge", "/usr/bin/google-chrome", "/usr/bin/chromium", "/usr/bin/chromium-browser",
    "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge", "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
  ].filter(Boolean);
  const executable = candidates.find(existsSync);
  assert.ok(executable, "Edge/Chrome not found. Set E2E_BROWSER to its executable path.");
  assert.ok(existsSync(path.join(root, "node_modules/vite/bin/vite.js")), "Run npm ci first.");
  profile = await mkdtemp(path.join(tmpdir(), "swapper-e2e-profile-"));
  if (capture) screenshots = await mkdtemp(path.join(tmpdir(), "swapper-e2e-screenshots-"));
  const vitePort = await freePort();
  origin = `http://127.0.0.1:${vitePort}`;
  const vite = launch(process.execPath, ["node_modules/vite/bin/vite.js", "--host", "127.0.0.1", "--port", String(vitePort), "--strictPort"]);
  await poll("Vite startup", async () => {
    if (vite.launchError || vite.exitCode !== null) throw new Error(vite.launchError?.message ?? vite.output);
    const response = await fetch(origin + fullApp, { signal: AbortSignal.timeout(500) });
    return response.ok;
  }, 12_000);
  const debugPort = await freePort();
  const endpoint = `http://127.0.0.1:${debugPort}`;
  const edge = launch(executable, ["--headless=new", `--user-data-dir=${profile}`, "--no-first-run", "--no-default-browser-check",
    "--disable-extensions", "--disable-background-networking", "--disable-component-update", "--disable-sync", "--disable-gpu",
    // In-process iframes: an out-of-process iframe's own navigations (the VOD controls switch) would bypass the page's Fetch interception and reach YouTube.
    "--disable-site-isolation-trials", "--disable-features=IsolateOrigins,site-per-process",
    "--remote-debugging-address=127.0.0.1", `--remote-debugging-port=${debugPort}`, "about:blank"]);
  const version = await poll("headless browser startup", async () => {
    if (edge.launchError || edge.exitCode !== null) throw new Error(edge.launchError?.message ?? edge.output);
    return getJson(endpoint + "/json/version");
  }, 8000);
  console.log(`${version.Browser} | ${origin} | 1280x820 | offline CDN fixtures`);
  browser = await Cdp.connect(version.webSocketDebuggerUrl);
  connections.push(browser);
  const { targetId } = await browser.send("Target.createTarget", { url: "about:blank" });
  const targets = await getJson(endpoint + "/json/list");
  const cdp = await Cdp.connect(targets.find(target => target.id === targetId).webSocketDebuggerUrl);
  connections.push(cdp);
  page = new Page(cdp);
  const record = issue => issues.push({ ...issue, page: current });
  cdp.on("Runtime.exceptionThrown", ({ exceptionDetails: e }) => record({ kind: "exception", text: e.exception?.description ?? e.text }));
  cdp.on("Runtime.consoleAPICalled", ({ type, args }) => {
    if (type === "error" || type === "assert") record({ kind: "console", text: args.map(arg => arg.value ?? arg.description ?? arg.type).join(" ") });
  });
  cdp.on("Log.entryAdded", ({ entry }) => {
    if (entry.level === "error") record({ kind: "log", text: entry.text, url: entry.url });
  });
  cdp.on("Network.responseReceived", ({ response }) => {
    if (response.status >= 400) record({ kind: "resource", url: response.url, status: response.status });
  });
  cdp.on("Fetch.requestPaused", ({ requestId, request }) => {
    void (async () => {
      const url = new URL(request.url);
      if (url.hostname === "i.ytimg.com") return cdp.send("Fetch.fulfillRequest", { requestId, responseCode: 200, responseHeaders: [{name: "Content-Type", value: "image/svg+xml"}], body: Buffer.from(placeholder).toString("base64") });
      if (url.origin === origin) return cdp.send("Fetch.continueRequest", { requestId });
      if (current === 'VOD player controls and persistent mini player' && url.origin === 'https://www.youtube-nocookie.com') {
        // An offline protocol peer tests our controls without depending on a live video.
        const html = `<html><body style="margin:0;background:#151515;color:white">Offline video test peer<script>
          const query = new URLSearchParams(location.search);
          let info = {currentTime:0,duration:1746,playerState:1,muted:false};
          const deliver = () => parent.postMessage(JSON.stringify({event:'infoDelivery',info}), ${JSON.stringify(origin)});
          const viewport = () => parent.postMessage(JSON.stringify({event:'testViewport',width:innerWidth,height:innerHeight,dpr:devicePixelRatio}), ${JSON.stringify(origin)});
          addEventListener('resize', viewport); viewport();
          addEventListener('message', e => {
            if (e.origin !== ${JSON.stringify(origin)}) return;
            const m = JSON.parse(e.data);
            if (m.event === 'command') parent.postMessage(JSON.stringify({event:'testCommand',func:m.func,args:m.args}), ${JSON.stringify(origin)});
            if (m.func === 'seekTo') info.currentTime = m.args[0];
            if (m.func === 'playVideo') info.playerState = 1;
            if (m.func === 'pauseVideo') info.playerState = 2;
            if (m.func === 'mute') info.muted = true;
            if (m.func === 'unMute') info.muted = false;
            deliver();
          });
        </script></body></html>`;
        return cdp.send('Fetch.fulfillRequest', { requestId, responseCode: 200, responseHeaders: [{ name: 'Content-Type', value: 'text/html' }], body: Buffer.from(html).toString('base64') });
      }
      if (!["ddragon.leagueoflegends.com", "raw.communitydragon.org"].includes(url.hostname)) {
        record({ kind: "network", text: `Unexpected external request: ${url.href}` });
        return cdp.send("Fetch.failRequest", { requestId, errorReason: "BlockedByClient" });
      }
      const json = url.pathname.endsWith("/runesReforged.json");
      await cdp.send("Fetch.fulfillRequest", { requestId, responseCode: 200,
        responseHeaders: [{ name: "Content-Type", value: json ? "application/json" : "image/svg+xml" }, { name: "Access-Control-Allow-Origin", value: "*" }],
        body: Buffer.from(json ? JSON.stringify(runes) : placeholder).toString("base64") });
    })().catch(error => record({ kind: "harness", text: error.message }));
  });
  await cdp.send("Page.enable");
  await cdp.send("Runtime.enable");
  await cdp.send("Log.enable");
  await cdp.send("Network.enable");
  await cdp.send("Fetch.enable", { patterns: [{ urlPattern: "http*" }] });
  await cdp.send("Emulation.setDeviceMetricsOverride", { width: 1280, height: 820, deviceScaleFactor: 1, mobile: false });

  await check("Dashboard preview", async () => { await navigate(fullApp, ".match-row"); await shell(); });
  await check('Location bar shows Dashboard and keeps empty title space draggable', async () => {
    await page.text('nav[aria-label="Location"] [aria-current="page"]', 'Dashboard');
    await page.text('.title-version', 'v1.0.17');
    assert.equal(await page.evaluate("document.querySelector('.title-location').hasAttribute('data-tauri-drag-region')"), false);
    assert.equal(await page.evaluate("document.querySelector('.title-spacer').hasAttribute('data-tauri-drag-region')"), true);
    assert.ok(await page.evaluate("document.querySelector('.title-spacer').getBoundingClientRect().width >= 32"));
  });
  if (capture) {
    await screenshot("dashboard-evidence", 1440, 900);
    await cdp.send("Emulation.setDeviceMetricsOverride", { width: 1280, height: 820, deviceScaleFactor: 1, mobile: false });
  }
  await check("Title bar offers a waiting update on every page", async () => {
    // The page from the previous checks has no update waiting; one navigation avoids a harness interception race.
    assert.equal(await page.evaluate("document.querySelector('.title-bar') !== null && document.querySelector('.update-pill') === null"), true);
    await navigate(`${fullApp}?update`, ".update-pill");
    await page.text(".update-pill", "Update to v1.0.4");
  });
  await check("LP graph shows gains, tier crests and a hover tooltip", async () => {
    await navigate(`${fullApp}?lp=month`, ".lp-graph svg");
    // Resize before hovering: a resize clears the hover the screenshot should show.
    if (capture) await page.cdp.send("Emulation.setDeviceMetricsOverride", { width: 1440, height: 900, deviceScaleFactor: 1, mobile: false });
    await page.text(".lp-graph-gains", "Last 30d");
    assert.ok(await page.evaluate("document.querySelectorAll('.lp-graph-guide .rank-crest, .lp-graph-guide .rank-crest-fallback').length >= 2"));
    const stops = await page.evaluate("Array.from(document.querySelector('.lp-graph linearGradient').children).map(s => ({ offset: s.offset.baseVal, color: s.style.stopColor }))");
    assert.equal(new Set(stops.map(s => s.color)).size, 3, "Platinum, Emerald and Diamond have their own line colors");
    assert.ok(stops.every((s, i) => !i || s.offset >= stops[i - 1].offset), "SVG gradient stops are ordered");
    const box = await page.evaluate("(() => { const r = document.querySelector('.lp-graph svg').getBoundingClientRect(); return { x: r.left + r.width / 2, y: r.top + r.height / 2 }; })()");
    await page.cdp.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: box.x, y: box.y });
    await page.text(".lp-graph-tip", "LP");
    if (capture) await screenshot("lp-graph-hover", 1440, 900);
    await page.cdp.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: 5, y: 5 });
    await page.wait("document.querySelector('.lp-graph-tip').hidden");
    await page.evaluate("document.querySelector('.lp-graph-plot').focus()");
    await page.text(".lp-graph-tip", "DIAMOND");
    const key = async (name, code) => {
      await page.cdp.send("Input.dispatchKeyEvent", { type: "keyDown", key: name, code: name, windowsVirtualKeyCode: code });
      await page.cdp.send("Input.dispatchKeyEvent", { type: "keyUp", key: name, code: name, windowsVirtualKeyCode: code });
      await page.settle();
    };
    await key("Home", 36);
    await page.text(".lp-graph-tip", "PLATINUM");
    await page.text(".lp-graph-tip", "Recorded rank");
    await key("ArrowRight", 39);
    await page.text(".lp-graph-tip", "1 game");
    await key("ArrowLeft", 37);
    await page.text(".lp-graph-tip", "Recorded rank");
    await key("End", 35);
    await page.text(".lp-graph-tip", "DIAMOND");
    for (const [width, height] of [[1900, 620], [1280, 820]]) {
      await page.cdp.send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: 1, mobile: false });
      await page.settle();
      assert.ok(await page.evaluate(`(() => {
        const graph = document.querySelector('.lp-graph').getBoundingClientRect();
        const plot = document.querySelector('.lp-graph-plot').getBoundingClientRect();
        const tip = document.querySelector('.lp-graph-tip').getBoundingClientRect();
        return plot.height >= 180 && tip.left >= graph.left && tip.right <= graph.right && tip.top >= graph.top && tip.bottom <= graph.bottom;
      })()`), `the plot has breathing room and its tooltip stays inside at ${width}px`);
      if (width === 1900) assert.ok(await page.evaluate(`(() => {
        const graph = document.querySelector('.lp-graph').getBoundingClientRect();
        const record = document.querySelector('.rank-record').getBoundingClientRect();
        return graph.left >= record.right;
      })()`), "the wide card places the graph in the third column");
      if (capture) await screenshot("lp-graph-keyboard", width, height);
    }
    await key("Escape", 27);
    await page.wait("document.querySelector('.lp-graph-tip').hidden");
    await page.evaluate("document.querySelector('.lp-graph-plot').blur()");
  });
  await check("Dashboard match opens and returns", async () => {
    await page.click(".match-champ");
    // ---- matchdetail: Overview is the default; context stays above every tab ----
    await page.visible(".md-page .md-comparison");
    await page.text(".ui-page-header h1", "Victory");
    await page.text('.location-trail button', 'Dashboard');
    await page.text('.location-trail [aria-current="page"]', 'Aatrox · Victory');
    assert.equal(await page.evaluate("document.querySelector('.ui-crumbs')"), null);
    assert.equal(await page.evaluate("document.querySelector('#md-tab-Overview').getAttribute('aria-selected')"), 'true');
    assert.deepEqual(await page.evaluate("[...document.querySelectorAll('.md-tabs button')].map(el => el.textContent)"), ['Overview', 'Scoreboard', 'Loadout', 'Stats', 'Graphs']);
    assert.equal(await page.evaluate("document.querySelectorAll('.md-team').length"), 0);
    await page.text('.md-context', 'Solo/Duo');
    // ---- end matchdetail default ----
    await audit();
    await screenshot("match-detail");
    // ---- matchdetail: comparison, loadout, searchable stats and timeline graph ----
    // The recap: stat ladder, ten damage dealers and the gold swing with objective markers.
    assert.ok(await page.evaluate("document.querySelectorAll('.md-recap-row').length >= 6"));
    assert.equal(await page.evaluate("document.querySelectorAll('.md-recap-dealer').length"), 10);
    assert.equal(await page.evaluate("document.querySelectorAll('.md-recap-bans img, .md-recap-bans .champion-icon-fallback').length"), 10);
    await page.visible('.md-recap-gold svg');
    if (capture) {
      await page.evaluate("document.querySelector('.md-recap').scrollIntoView()");
      await screenshot("match-recap", 1440, 900);
    }
    assert.equal(await page.evaluate("document.querySelectorAll('.md-fact').length"), 3);
    assert.equal(await page.evaluate("document.querySelector('.md-tower-reading span.is-blue').textContent"), 'Your team');
    assert.ok(await page.evaluate("document.querySelector('.md-share-ring').getAttribute('aria-label').includes('damage to champions')"));
    await page.click('#md-tab-Scoreboard');
    assert.equal(await page.evaluate("document.querySelectorAll('.md-player:not([aria-hidden])').length"), 10);
    await page.text('.md-rank', 'Master 200 LP');
    await page.text('.md-rank', 'Diamond II 45 LP');
    assert.ok(await page.evaluate("document.querySelector('.md-rank').getAttribute('aria-label').startsWith('Current Solo/Duo rank:')"));
    assert.equal(await page.evaluate("document.querySelector('.md-header, .md-comparison')"), null);
    // A tab switch scrolls to the page bottom, but never past the tabs reaching the top.
    await page.evaluate("document.querySelector('.dashboard-main').scrollTop = 0");
    await page.evaluate("document.querySelector('#md-tab-Loadout').click()");
    await page.visible('.md-runes');
    await page.wait("(() => { const s = document.querySelector('.dashboard-main'), tabs = document.querySelector('.md-tabs').getBoundingClientRect().top - s.getBoundingClientRect().top; return s.scrollTop > 0 && (Math.abs(s.scrollTop + s.clientHeight - s.scrollHeight) < 2 || Math.abs(tabs) < 2); })()", 'tab switch scrolls the new panel into view');
    assert.equal(await page.evaluate("document.querySelectorAll('.md-player .md-items .ui-slot').length"), 70);
    await page.click('#md-tab-Stats');
    await page.visible('.md-stats-table');
    assert.equal(await page.evaluate("document.querySelectorAll('.md-stats-group').length"), 6);
    assert.equal(await page.evaluate("document.querySelector('#md-group-Loadout-data, #md-group-Other-client-stats')"), null);
    await page.text('.md-stats-meta', 'across 6 groups');
    assert.equal(await page.evaluate("document.querySelectorAll('.md-stats-portraits th:not(:first-child)').length"), 10);
    assert.equal(await page.evaluate("document.querySelectorAll('.md-stats-portraits .is-me').length"), 1);
    assert.ok(await page.evaluate("document.querySelectorAll('.md-stats-table .is-best').length > 0"));
    await page.click('.md-stats-group button[aria-controls="md-group-Combat"]');
    assert.equal(await page.evaluate("document.querySelector('#md-group-Combat').hidden"), true);
    await page.evaluate("document.querySelector('.md-stats-scroll').scrollTop = 300");
    await page.settle();
    assert.ok(await page.evaluate("Math.abs(document.querySelector('.md-stats-sides th').getBoundingClientRect().top - document.querySelector('.md-stats-scroll').getBoundingClientRect().top) < 2"), 'stats header stays inside its scrolling card');
    await page.evaluate("document.querySelector('.md-stats-scroll').scrollTop = 0");
    await page.type('.md-stat-search input', 'vision');
    await page.wait("document.querySelectorAll('.md-stats-table tbody:not(.md-stats-group) tr').length === 5", 'five vision stats');
    for (const [width, height] of [[1440, 900], [1920, 1080], [2560, 1440], [1280, 820]]) {
      await cdp.send('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 1, mobile: false });
      await audit();
    }
    await page.click('#md-tab-Graphs');
    await page.visible('.md-chart polyline');
    assert.equal(await page.evaluate("document.querySelectorAll('.md-event-list button').length"), 10);
    assert.equal(await page.evaluate("document.querySelectorAll('.md-lane-mark.is-kill').length"), 8);
    assert.equal(await page.evaluate("document.querySelectorAll('.md-chart .md-chart-event, .md-kill-track').length"), 0);
    await page.click('.md-event-filters button', 'Objectives');
    assert.equal(await page.evaluate("document.querySelectorAll('.md-event-list button').length"), 6);
    assert.equal(await page.evaluate("document.querySelectorAll('.md-lane-mark.is-kill').length"), 8);
    await page.click('.md-event-filters button', 'Kills');
    assert.equal(await page.evaluate("document.querySelectorAll('.md-event-list button').length"), 8);
    assert.equal(await page.evaluate("document.querySelectorAll('.md-chart-event').length"), 0);
    await page.click('.md-event-filters button', 'All');
    assert.equal(await page.evaluate("document.querySelectorAll('.md-event-list button').length"), 12);
    await page.click('.md-event-expand button');
    assert.equal(await page.evaluate("document.querySelectorAll('.md-event-list button').length"), 14);
    await page.click('.md-chart-select input');
    await cdp.send('Input.dispatchKeyEvent', { type: 'keyDown', key: 'Home', code: 'Home' });
    await cdp.send('Input.dispatchKeyEvent', { type: 'keyUp', key: 'Home', code: 'Home' });
    await page.text('.md-chart-reading', 'Teams even');
    await page.visible('.md-timeline-popover');
    await page.text('.md-timeline-popover', 'Teams even at 0:00');
    await cdp.send('Input.dispatchKeyEvent', { type: 'keyDown', key: 'ArrowRight', code: 'ArrowRight', windowsVirtualKeyCode: 39, nativeVirtualKeyCode: 39 });
    await cdp.send('Input.dispatchKeyEvent', { type: 'keyUp', key: 'ArrowRight', code: 'ArrowRight', windowsVirtualKeyCode: 39, nativeVirtualKeyCode: 39 });
    await page.text('.md-timeline-popover', 'at 1:00');
    await page.evaluate("document.querySelector('.dashboard-main').scrollTop = 10000; document.querySelector('#md-tab-Scoreboard').click()");
    await page.wait("Math.abs(document.querySelector('.md-tabs').getBoundingClientRect().top - document.querySelector('.dashboard-main').getBoundingClientRect().top) < 2", 'offscreen tabs return to viewport top');
    await cdp.send('Emulation.setEmulatedMedia', {features: [{name: 'prefers-reduced-motion', value: 'reduce'}]});
    await page.evaluate("document.querySelector('.dashboard-main').scrollTop = 10000; document.querySelector('#md-tab-Graphs').click()");
    await page.wait("Math.abs(document.querySelector('.md-tabs').getBoundingClientRect().top - document.querySelector('.dashboard-main').getBoundingClientRect().top) < 2", 'reduced-motion tab position');
    // Keyboard activation keeps focus on the selected tab without a second browser scroll.
    await page.evaluate("document.querySelector('#md-tab-Graphs').focus({preventScroll: true})");
    await key('Home');
    await page.wait("document.querySelector('#md-tab-Overview').getAttribute('aria-selected') === 'true'", 'Home selects Overview');
    await key('ArrowRight');
    await page.wait("document.querySelector('#md-tab-Scoreboard').getAttribute('aria-selected') === 'true'", 'ArrowRight selects Scoreboard');
    await cdp.send('Emulation.setEmulatedMedia', {features: []});
    for (const [width, height] of [[1280, 820], [1920, 1080], [2560, 1440]]) {
      await cdp.send('Emulation.setDeviceMetricsOverride', {width, height, deviceScaleFactor: 1, mobile: false});
      await audit();
      assert.ok(await page.evaluate("parseFloat(getComputedStyle(document.querySelector('.md-rank')).fontSize) >= 14"));
      assert.ok(await page.evaluate("parseFloat(getComputedStyle(document.querySelector('.md-identity > strong')).fontSize) >= 15"));
    }
    await cdp.send('Emulation.setDeviceMetricsOverride', {width: 1280, height: 820, deviceScaleFactor: 1, mobile: false});
    // ---- end matchdetail tabs ----
    await page.click('.location-trail button', "Dashboard");
    await page.visible(".match-row.is-last");
  });
  // ---- matchdetail: unavailable timeline and absent stats never become fake zeroes ----
  // A row from the previous Dashboard can still be visible before navigation commits.
  async function matchPreview(query = '') {
    await navigate(fullApp + query, '.match-row');
    await page.wait(`location.search === ${JSON.stringify(query)} && document.readyState === 'complete'`, 'match preview navigation committed');
  }
  await check('Match stats with unavailable fields and timeline', async () => {
    await matchPreview('?no-timeline&detail-missing');
    await page.click('.match-champ');
    await page.visible('.md-comparison');
    assert.equal(await page.evaluate("document.querySelector('#md-tab-Graphs')"), null);
    await page.click('#md-tab-Stats');
    await page.type('.md-stat-search input', 'visionScore');
    await page.wait("document.querySelectorAll('.md-stats-table tbody:not(.md-stats-group) tr').length === 1", 'vision score only');
    assert.equal(await page.evaluate("document.querySelector('.md-stats-table tbody:not(.md-stats-group) tr td:nth-of-type(3)').textContent"), '–');
    assert.equal(await page.evaluate("document.querySelectorAll('.md-stats-table .is-best').length"), 0);
    await page.type('.md-stat-search input', 'no-such-stat');
    await page.visible('.md-stats-empty');
    await page.click('.md-stats-empty button');
    await page.wait("document.querySelectorAll('.md-stats-table tbody:not(.md-stats-group) tr').length > 40", 'search cleared');
    await matchPreview();
  });
  await check('Match graphs: dense lanes, kill map, player filters and keyboard', async () => {
    await matchPreview('?timeline-long');
    await page.click('.match-champ');
    await page.click('#md-tab-Graphs');
    await page.visible('.md-map-dot');
    assert.equal(await page.evaluate("document.querySelectorAll('.md-map-dot').length"), 72);
    assert.equal(await page.evaluate("document.querySelectorAll('.md-frame-dot').length"), 35);
    assert.equal(await page.evaluate("document.querySelectorAll('.md-champion-filters button').length"), 10);
    assert.equal(await page.evaluate("document.querySelectorAll('.md-event-lane').length"), 2);
    assert.equal(await page.evaluate("document.querySelectorAll('.md-lane-mark.is-kill').length"), 24);
    assert.equal(await page.evaluate("document.querySelectorAll('.md-event-list button').length"), 12);
    assert.equal(await page.evaluate("document.querySelector('.md-event-filters button').getAttribute('aria-pressed')"), 'true');
    await page.click('.md-champion-filters button.is-me');
    const personal = await page.evaluate("document.querySelectorAll('.md-map-dot').length");
    assert.ok(personal > 0 && personal < 72);
    assert.equal(await page.evaluate("document.querySelectorAll('.md-map-dot:not(.is-personal)').length"), 0);
    assert.equal(await page.evaluate("document.querySelectorAll('.md-lane-mark.is-kill:not(.is-personal)').length"), 0);
    assert.equal(await page.evaluate("document.querySelectorAll('.md-lane-mark.is-objective').length"), 18);
    for (const [width, height] of [[1440, 900], [2560, 1440], [1280, 820]]) {
      await cdp.send('Emulation.setDeviceMetricsOverride', {width, height, deviceScaleFactor: 1, mobile: false});
      assert.equal(await page.evaluate("getComputedStyle(document.querySelector('.md-graphs-layout')).gridTemplateColumns.split(' ').length"), width >= 1440 ? 2 : 1);
      await audit();
    }
    await page.evaluate("document.querySelector('.md-lane-mark.is-kill').focus()");
    await page.visible('#md-event-tooltip');
    assert.equal(await page.evaluate("document.querySelectorAll('#md-event-tooltip .md-kill-portrait').length > 0"), true);
    await cdp.send('Input.dispatchKeyEvent', {type: 'keyDown', key: 'Escape', code: 'Escape'});
    await page.wait("document.querySelector('#md-event-tooltip') === null", 'event tooltip dismissed');
    await page.click('.md-map-filter-heading button');
    assert.equal(await page.evaluate("document.querySelectorAll('.md-map-dot').length"), 72);
    // Hovering the graph narrows the map to that minute; a click keeps it after the pointer leaves.
    const chart = await page.evaluate("(() => { const el = document.querySelector('.md-chart'); el.scrollIntoView({ block: 'center', behavior: 'instant' }); const r = el.getBoundingClientRect(); return { x: r.x + r.width * .6, y: r.y + r.height / 2 }; })()");
    await cdp.send('Input.dispatchMouseEvent', {type: 'mouseMoved', ...chart});
    await page.visible('.md-map-minute');
    const minute = await page.evaluate("document.querySelectorAll('.md-map-dot').length");
    assert.ok(minute < 72);
    await cdp.send('Input.dispatchMouseEvent', {type: 'mousePressed', ...chart, button: 'left', clickCount: 1});
    await cdp.send('Input.dispatchMouseEvent', {type: 'mouseReleased', ...chart, button: 'left', clickCount: 1});
    await page.visible('.md-frame-dot.is-pinned');
    await cdp.send('Input.dispatchMouseEvent', {type: 'mouseMoved', x: 2, y: 2});
    await page.wait("document.querySelector('#md-minute-tooltip') === null", 'minute tooltip closed');
    assert.equal(await page.evaluate("document.querySelectorAll('.md-map-dot').length"), minute);
    await page.click('.md-map-minute button', 'Show all');
    assert.equal(await page.evaluate("document.querySelectorAll('.md-map-dot').length"), 72);
    await page.click('.md-event-filters button', 'All');
    await page.click('.md-event-expand button');
    assert.equal(await page.evaluate("document.querySelectorAll('.md-event-list button').length"), 90);
    await page.click('.md-event-expand button');
    assert.equal(await page.evaluate("document.querySelectorAll('.md-event-list button').length"), 12);
    await matchPreview('?timeline-long&timeline-no-positions');
    await page.click('.match-champ');
    await page.click('#md-tab-Graphs');
    await page.visible('.md-event-lane');
    assert.equal(await page.evaluate("document.querySelector('.md-kill-map')"), null);
    assert.equal(await page.evaluate("document.querySelectorAll('.md-lane-mark.is-kill').length"), 24);
    await matchPreview();
  });
  await check('Match current ranks: Flex, fallback, unranked and missing LP', async () => {
    await matchPreview('?detail-flex&detail-rank-fallback&detail-unranked&detail-rank-missing-lp');
    await page.click('.match-champ');
    await page.click('#md-tab-Scoreboard');
    await page.text('.md-context', 'Flex');
    const ranks = await page.evaluate("[...document.querySelectorAll('.md-rank')].map(el => ({label: el.textContent, tooltip: el.dataset.tooltip}))");
    assert.equal(ranks[0].tooltip, 'Current Flex rank');
    assert.equal(ranks[1].label, 'Unranked');
    assert.equal(ranks[2].tooltip, 'Current Solo/Duo rank');
    assert.equal(ranks[2].label, 'Diamond II 45 LP');
    assert.ok(!ranks[3].label.includes('LP'), 'missing LP stays missing');
    await page.click('.location-trail button', 'Dashboard');
    await page.click('.match-row:nth-child(2) .match-champ');
    await page.wait("document.querySelector('#md-tab-Overview').getAttribute('aria-selected') === 'true'", 'another match resets Overview');
    await matchPreview('?slow-ranks');
    await page.click('.match-champ');
    await page.visible('#md-tab-Overview');
    await page.text('.ui-notice', 'Loading ranks');
    await page.click('#md-tab-Scoreboard');
    await page.text('.md-rank', 'Rank unavailable');
    await page.text('.md-rank', 'Master 200 LP');
    await matchPreview();
  });
  // ---- end matchdetail missing data ----
  await check("Sidebar Tierlist", async () => {
    await nav("Tierlist", ".tier-table tbody tr");
    assert.equal(await page.evaluate('document.querySelector(".nav-badge.patch").dataset.label'), "PATCH 26.21");
    assert.equal(await page.evaluate('document.querySelector(".nav-badge.patch").dataset.tooltip'), "Patch 26.21");
  });
  await check("Shared search pill focuses its rounded container", searchPillFocus);
  await check("Tierlist champion overview and build", async () => {
    await page.click(".tier-champ", "Aatrox");
    await page.text(".champion-hero h2", "Aatrox");
    await page.text(".champion-hero .fa-eyebrow", "Patch 26.21");
    await page.text('.location-trail button', 'Tierlist');
    await page.text('.location-trail [aria-current="page"]', 'Aatrox');
    await page.visible(".fa-build-section");
    await audit();
    await screenshot("champion-overview");
    await page.evaluate("document.querySelector('.fa-build-section').scrollIntoView()");
    await screenshot("champion-overview-wide", 1600, 900);
    await page.click('.location-trail button', 'Tierlist');
    await page.visible(".tier-table tbody tr");
    await page.text('.location-trail [aria-current="page"]', 'Tierlist');
  });
  await check("Tierlist Duos groups paired portraits and filters by either champion", async () => {
    await page.click('.role-tabs button', 'Duos');
    await page.visible('.fa-duo-table tbody tr');
    await page.wait("Array.from(document.querySelectorAll('.fa-duo-table tbody tr')).every(row => row.querySelectorAll('img.champion-icon').length === 2)", 'two champion portraits per duo');
    await page.text('.fa-duo-table tbody tr:first-child', 'Jinx');
    await page.text('.fa-duo-table tbody tr:first-child', 'Milio');
    await page.text('.fa-duo-table tbody tr:first-child', '54.8%');
    for (const width of [1280, 1650, 1920]) {
      await cdp.send('Emulation.setDeviceMetricsOverride', { width, height: 900, deviceScaleFactor: 1, mobile: false });
      await page.settle();
      assert.deepEqual(await page.evaluate(`(${horizontalOverflow.toString()})()`), [], `${width}px duo layout`);
      const pair = await page.evaluate("(() => { const row = document.querySelector('.fa-duo-table tbody tr'); const group = row.querySelector('.fa-duo-portraits'); return { portraits: [...row.querySelectorAll('img')].map(img => img.getBoundingClientRect().width), groupWidth: group?.getBoundingClientRect().width }; })()");
      assert.ok(pair.portraits.every(width => width >= 32) && pair.groupWidth < 100, `${width}px: portraits must read as a compact pair`);
      if (width !== 1650) await screenshot('tierlist-duos-second-pass', width, 900);
    }
    await cdp.send('Emulation.setDeviceMetricsOverride', { width: 1280, height: 820, deviceScaleFactor: 1, mobile: false });
    await page.click('.fa-duo-table tbody tr:first-child .col-wr');
    assert.equal(await page.evaluate("document.querySelector('.champion-hero')"), null);
    await page.text('.role-tabs button[aria-pressed="true"]', 'Duos');
    await page.click('button[aria-label="Show duos with Ziggs"]');
    await page.text('.fa-duo-filter', 'With Ziggs');
    await page.wait("document.querySelectorAll('.fa-duo-table tbody tr').length === 2", 'Ziggs bot pairs');
    await page.click('button[aria-label="Show duos with Ziggs"]');
    await page.wait("document.querySelectorAll('.fa-duo-table tbody tr').length === 17", 'same image clears filter');
    await page.click('button[aria-label="Show duos with Milio"]');
    await page.text('.fa-duo-filter', 'With Milio');
    await page.wait("document.querySelectorAll('.fa-duo-table tbody tr').length === 5", 'Milio support pairs');
    await page.click('button[aria-label="Show duos with Ziggs"]');
    await page.text('.fa-duo-filter', 'With Ziggs');
    await page.type('[aria-label="Search duo"]', 'mIlIo');
    await page.wait("document.querySelectorAll('.fa-duo-table tbody tr').length === 1", 'champion filter combines with search');
    await page.click('.fa-duo-table th button', 'Games together');
    await page.text('.fa-duo-table tbody tr', 'Ziggs');
    await page.click('button[aria-label="Clear champion filter"]');
    await page.wait("document.querySelectorAll('.fa-duo-table tbody tr').length === 5", 'chip clears only champion filter');
    await page.evaluate("document.querySelector('[aria-label=\"Search duo\"]').select()");
    await cdp.send('Input.dispatchKeyEvent', { type: 'keyDown', key: 'Backspace', windowsVirtualKeyCode: 8 });
    await cdp.send('Input.dispatchKeyEvent', { type: 'keyUp', key: 'Backspace', windowsVirtualKeyCode: 8 });
    await page.click('.role-tabs button', 'Auto');
    await page.visible('.tier-table tbody tr');
  });
  await check("Tierlist Duos supports search, rank changes, sorting and keyboard selection", async () => {
    await page.evaluate("[...document.querySelectorAll('.role-tabs button')].find(button => button.textContent === 'Duos').focus()");
    await cdp.send('Input.dispatchKeyEvent', { type: 'keyDown', key: 'Enter', code: 'Enter', text: '\r', windowsVirtualKeyCode: 13 });
    await cdp.send('Input.dispatchKeyEvent', { type: 'keyUp', key: 'Enter', code: 'Enter', windowsVirtualKeyCode: 13 });
    await page.visible('.fa-duo-table tbody tr');
    for (const [query, count] of [['mIlIo', 5], ['Jinx', 2], ['no such duo', 0]]) {
      await page.evaluate("document.querySelector('[aria-label=\"Search duo\"]').select()");
      await cdp.send('Input.insertText', { text: query });
      await page.wait(`document.querySelectorAll('.fa-duo-table tbody tr').length === ${count}`, `duo search ${query}`);
    }
    await page.text('.panel-empty', 'No duo matches');
    await page.evaluate("document.querySelector('[aria-label=\"Search duo\"]').select()");
    await cdp.send('Input.dispatchKeyEvent', { type: 'keyDown', key: 'Backspace', windowsVirtualKeyCode: 8 });
    await cdp.send('Input.dispatchKeyEvent', { type: 'keyUp', key: 'Backspace', windowsVirtualKeyCode: 8 });
    await page.wait("document.querySelectorAll('.fa-duo-table tbody tr').length === 17", 'cleared duo search');
    await page.click('.fa-duo-table th button', 'Games together');
    await page.text('.fa-duo-table tbody tr:nth-child(2)', 'Jinx');
    await page.click('.fa-duo-table th button', 'Rank');
    await page.text('.fa-duo-table tbody tr:nth-child(2)', 'Vayne');
    await page.click('button[aria-label^="Rank:"]');
    await page.click('.fa-picker-option', 'Diamond+');
    await page.text('.fa-duo-table tbody tr:first-child', '53.9%');
    await page.text('.fa-duo-table tbody tr:first-child', '9,210');
    await page.evaluate("document.querySelector('button[aria-label=\"Show duos with Jinx\"]').focus()");
    await cdp.send('Input.dispatchKeyEvent', { type: 'keyDown', key: 'Enter', code: 'Enter', text: '\r', windowsVirtualKeyCode: 13 });
    await cdp.send('Input.dispatchKeyEvent', { type: 'keyUp', key: 'Enter', code: 'Enter', windowsVirtualKeyCode: 13 });
    await page.text('.fa-duo-filter', 'With Jinx');
    await page.wait("document.querySelectorAll('.fa-duo-table tbody tr').length === 2", 'keyboard champion filter');
    assert.equal(await page.evaluate("document.querySelector('.champion-hero')"), null);
    await page.evaluate("document.querySelector('button[aria-label=\"Clear champion filter\"]').focus()");
    await cdp.send('Input.dispatchKeyEvent', { type: 'keyDown', key: 'Enter', code: 'Enter', text: '\r', windowsVirtualKeyCode: 13 });
    await cdp.send('Input.dispatchKeyEvent', { type: 'keyUp', key: 'Enter', code: 'Enter', windowsVirtualKeyCode: 13 });
    await page.wait("!document.querySelector('.fa-duo-filter')", 'keyboard clears chip');
  });
  await check("Tierlist Duos searches every pair and pages beyond the first forty", async () => {
    await navigate(`${fullApp}?duos=paged`, '.match-row');
    await nav('Tierlist', '.tier-table tbody tr');
    await page.click('.role-tabs button', 'Duos');
    await page.wait("document.querySelectorAll('.fa-duo-table tbody tr').length === 40", 'first forty pairs');
    await page.type('[aria-label="Search duo"]', 'Smolder');
    await page.wait("document.querySelectorAll('.fa-duo-table tbody tr').length === 4", 'search includes pairs beyond first forty');
    await page.evaluate("document.querySelector('[aria-label=\"Search duo\"]').select()");
    await cdp.send('Input.dispatchKeyEvent', { type: 'keyDown', key: 'Backspace', windowsVirtualKeyCode: 8 });
    await cdp.send('Input.dispatchKeyEvent', { type: 'keyUp', key: 'Backspace', windowsVirtualKeyCode: 8 });
    await page.wait("document.querySelectorAll('.fa-duo-table tbody tr').length === 40", 'search reset starts at forty');
    await page.evaluate("document.querySelector('.fa-duo-table + [aria-hidden]').scrollIntoView({block: 'center'})");
    await page.wait("document.querySelectorAll('.fa-duo-table tbody tr').length === 48", 'remaining pairs load on scroll');
  });
  await check("Tierlist Duos handles loading, empty, unavailable, cached and failed data", async () => {
    for (const state of ['loading', 'empty', 'unavailable', 'stale', 'error']) {
      await navigate(`${fullApp}?duos=${state}`, '.match-row');
      await nav('Tierlist', '.tier-table tbody tr');
      await page.click('.role-tabs button', 'Duos');
      if (state === 'loading') {
        await page.visible('[role="status"][aria-label="Loading bot-lane duos"]');
        await page.visible('.fa-duo-table tbody tr');
      } else if (state === 'empty') await page.text('.panel-empty', 'No bot-lane duo data for this rank yet.');
      else if (state === 'unavailable') await page.text('.ui-notice', 'Duos unavailable');
      else if (state === 'error') await page.text('[role="alert"]', 'Could not load bot-lane duos');
      else { await page.visible('.fa-duo-table tbody tr'); await page.text('.cached-badge', 'Cached'); }
      await audit();
    }
    await navigate(fullApp, '.match-row');
  });
  await check("Sidebar Matchups", async () => {
    await nav("Matchups", ".fa-matchup-empty");
    assert.equal(await page.evaluate("[...document.querySelectorAll('.role-tabs button')].some(button => button.textContent === 'Duos')"), false);
  });
  await check('All-champion VOD browser defaults, paging, rows, clearing and side visibility', async () => {
    await navigate(fullApp, '.match-row');
    await page.evaluate(`(() => {
      const invoke = window.__TAURI_INTERNALS__.invoke;
      const base = {title:'League replay',rankClaim:'Master',patch:'26.21',durationSec:1500,moments:[],lane:'mid'};
      const all = Array.from({length:125},(_,i)=>({...base,videoId:'all-vod-'+i,champion:i<64||i===124?103:238,enemy:i<64?238:i===124?157:103,lane:i===124?'top':'mid',patch:i===124?'26.22':'26.21'}));
      window.__vodCalls = [];
      window.__TAURI_INTERNALS__.invoke = (cmd,args) => {
        if(cmd==='get_all_vods') {window.__vodCalls.push(cmd);return Promise.resolve(all);}
        if(cmd==='get_champion_vods') return Promise.resolve(all.filter(v=>v.champion===args.championId));
        return invoke(cmd,args);
      };
    })()`);
    const choose = async (label, prefix) => {
      await page.click(`button[aria-label^="${label}:"]`);
      await page.click('.fa-picker-option', await page.evaluate(`[...document.querySelectorAll('.fa-picker-option')].map(b=>b.textContent.trim()).find(t=>t.startsWith(${JSON.stringify(prefix)}))`));
    };
    await nav('VODs', '.mv-vod-row');
    await page.visible('button[aria-label="Champion: All champions"]');
    await page.visible('button[aria-label="Opponent: Any opponent"]');
    await page.text('.mv-match-count', '125 VODs');
    assert.equal(await page.evaluate("!!document.querySelector('.mv-banner-art')"), false, 'no champion uses a neutral banner');
    await page.text('.mv-champion-picker', 'Choose champion');
    await page.text('.mv-player-tabs', 'Pros');
    assert.equal(await page.evaluate("document.querySelector('.mv-player-tabs button:last-child').dataset.tooltip"), 'Pros and well-known streamers');
    await page.wait("document.querySelectorAll('.mv-vod-row').length===60", 'all-champion library starts with sixty rows');
    assert.equal(await page.evaluate("document.querySelector('.mv-vod-row').dataset.vodId"), 'all-vod-124@0', 'newest patch first across champions');
    await page.text('.mv-vod-row:first-child .mv-cell-matchup', 'vs Yasuo');
    await page.text('.mv-vod-row:first-child .mv-cell-matchup', 'Top');
    assert.equal(await page.evaluate("document.querySelector('.mv-vod-row:first-child .mv-cell-matchup').querySelectorAll('.champion-icon, .champion-icon-fallback').length"), 2, 'both champions have portraits');
    assert.equal(await page.evaluate("!!document.querySelector('[aria-label=Perspective]')"), false);
    await page.evaluate("document.querySelector('.mv-vod-list').scrollTop=1e6");
    await page.wait("document.querySelectorAll('.mv-vod-row').length===120", 'second page across champions');
    await page.evaluate("document.querySelector('.mv-vod-list').scrollTop=1e6");
    await page.wait("document.querySelectorAll('.mv-vod-row').length===125", 'last page across champions');
    await choose('Opponent', 'Zed');
    await page.wait("document.querySelectorAll('.mv-vod-row').length===60", 'opponent-only filter resets paging');
    assert.equal(await page.evaluate("!!document.querySelector('[aria-label=Perspective]')"), false, 'opponent alone has no reverse toggle');
    await page.click('.mv-matchup-scope button', 'Clear filters');
    await choose('Champion', 'Ahri');
    await page.wait("document.querySelector('.mv-match-count').textContent==='65 VODs'", 'champion filter');
    await page.visible('.mv-banner-art');
    await page.text('.mv-champion-picker', 'Ahri');
    const banner = await page.evaluate("document.querySelector('.mv-filters').getBoundingClientRect().height");
    assert.ok(banner >= 140 && banner <= 170, `desktop banner height: ${banner}`);
    await page.click('.mv-lanes button[aria-label="Top"]');
    await page.text('.mv-match-count', '1 VODs');
    await page.visible('.mv-lanes button[aria-label="Top"][aria-pressed="true"]');
    for (const type of ['keyDown', 'keyUp']) await cdp.send('Input.dispatchKeyEvent', { type, key: ' ', code: 'Space', windowsVirtualKeyCode: 32 });
    await page.text('.mv-match-count', '65 VODs');
    await page.evaluate("document.querySelector('.mv-filter-chips button[aria-label^=\"Rank:\"]').focus()");
    for (const type of ['keyDown', 'keyUp']) await cdp.send('Input.dispatchKeyEvent', { type, key: 'Enter', code: 'Enter', windowsVirtualKeyCode: 13, ...(type === 'keyDown' ? { text: '\r' } : {}) });
    await page.visible('.fa-picker-popover[aria-label="Rank"]');
    for (const type of ['keyDown', 'keyUp']) await cdp.send('Input.dispatchKeyEvent', { type, key: 'Escape', code: 'Escape', windowsVirtualKeyCode: 27 });
    await page.wait("!document.querySelector('.fa-picker-popover')", 'rank menu closes with Escape');
    await cdp.send('Emulation.setDeviceMetricsOverride', { width: 800, height: 700, deviceScaleFactor: 1, mobile: false });
    await page.settle();
    assert.ok(await page.evaluate("(() => { const el = document.querySelector('.mv-filters'); return el.scrollWidth <= el.clientWidth && [...el.querySelectorAll('button')].every(b => b.getBoundingClientRect().right <= el.getBoundingClientRect().right); })()"), 'banner controls wrap in a narrow window');
    await cdp.send('Emulation.setDeviceMetricsOverride', { width: 1280, height: 820, deviceScaleFactor: 1, mobile: false });
    assert.equal(await page.evaluate("!!document.querySelector('[aria-label=Perspective]')"), false, 'champion alone has no reverse toggle');
    await choose('Opponent', 'Zed');
    await page.visible('.mv-segment[aria-label="Perspective"]');
    await page.click('.mv-segment[aria-label="Perspective"] button:last-child');
    await page.text('.mv-vod-list[aria-label="From Zed’s side"]', 'vs Ahri');
    await choose('Champion', 'All champions');
    await page.visible('button[aria-label="Champion: All champions"]');
    await page.text('.mv-match-count', '125 VODs');
    await page.visible('button[aria-label="Opponent: Any opponent"]');
    assert.equal(await page.evaluate("!!document.querySelector('[aria-label=Perspective]')"), false, 'clearing champion clears reverse mode');
    await choose('Champion', 'Ahri');
    await nav('Dashboard', '.match-row');
    await nav('VODs', '.mv-vod-row');
    await page.visible('button[aria-label="Champion: All champions"]');
    await page.text('.mv-match-count', '125 VODs');
    assert.deepEqual(await page.evaluate('window.__vodCalls'), ['get_all_vods'], 'changing filters reuses the aggregate library');
  });
  await check('VODs section champion pick, matchup entry, perspectives and library rules', async () => {
    await navigate(fullApp, '.match-row');
    await page.evaluate(`(() => {
      const invoke = window.__TAURI_INTERNALS__.invoke;
      const base = {title:'Ahri vs Zed',rankClaim:'Grandmaster',region:'KR',patch:'26.21',durationSec:1500,moments:[],enemy:238,lane:'mid'};
      const own = [
        {...base,videoId:'anonymous01',uploaded:'2026-11-02'},
        {...base,videoId:'named-old01',player:'Faker',uploaded:'2026-10-01'},
        {...base,videoId:'named-new01',player:'Ruler',uploaded:'2026-11-01',patchEstimated:true},
        {...base,videoId:'named-tie01',player:'Peyz',uploaded:'2026-11-01'},
        {...base,videoId:'patch-new01',patch:'26.22'},
        ...Array.from({length:3},(_,i)=>({...base,videoId:'match-vod0'+i,patch:'26.20'})),
        ...Array.from({length:5},(_,i)=>({...base,videoId:'other-vod0'+i,enemy:157,lane:'top',player:i===0?'Solo player':undefined,patch:'26.20'}))
      ];
      const reverse = [{...base,videoId:'reverse-mid',enemy:103,player:'Zed player'}, {...base,videoId:'reverse-top',enemy:103,lane:'top'}];
      window.__TAURI_INTERNALS__.invoke = (cmd,args) => cmd === 'get_all_vods' ? Promise.resolve([...own.map(v=>({...v,champion:103})),...reverse.map(v=>({...v,champion:238}))])
        : cmd === 'get_champion_vods' ? Promise.resolve(args.championId===103?own:args.championId===238?reverse:[])
        : cmd === 'get_matchup_vods' ? Promise.resolve(args.championId===103 && args.enemyChampionId===238 ? own.filter(v=>v.enemy===238&&v.lane===args.position):[])
        : cmd === 'player_photo' ? Promise.reject('No fixture photo') : invoke(cmd,args);
    })()`);
    await nav('Matchups', '.fa-matchup-empty');
    await page.click('.fa-champion-card[data-champion-id="103"]');
    await page.click('.fa-champion-card[data-champion-id="238"]');
    await page.text('.mv-entry', '8 VODs for Ahri vs Zed');
    assert.equal(await page.evaluate("[...document.querySelectorAll('.mg-workspace-tabs button')].some(b=>b.textContent==='VODs')"), false);
    await page.click('.mv-entry button', 'Watch');
    await page.text('nav button[aria-current="page"]', 'VODs');
    await page.click('button[aria-label="Back"]');
    await page.visible('.mg-guide-view');
    await page.text('.mg-context', 'Ahri vs Zed');
    await page.click('.mg-context button', await page.evaluate("[...document.querySelectorAll('.mg-context button')].find(b=>b.textContent.includes('Watch')).textContent"));
    await page.text('.mv-matchup-scope', 'Ahri vs Zed · Mid');
    await page.wait("document.querySelectorAll('.mv-vod-row').length===8", 'entry filters champion, opponent and lane');
    assert.deepEqual(await page.evaluate("[...document.querySelectorAll('.mv-vod-row')].map(r=>r.dataset.vodId)"), ['patch-new01@0','named-new01@0','named-tie01@0','named-old01@0','anonymous01@0','match-vod00@0','match-vod01@0','match-vod02@0']);
    await page.text('.mv-cell-patch', '26.21');
    await page.text('.mv-cell-patch [data-tooltip="Estimated from the upload date"]', 'est.');
    await page.text('.mv-vod-row[data-vod-id="anonymous01@0"] strong', 'Ranked replay');
    await page.click('.mv-segment[aria-label="Players"] button:last-child');
    await page.wait("document.querySelectorAll('.mv-vod-row').length===3", 'named players include people without teams');
    assert.ok(await page.evaluate("[...document.querySelectorAll('.mv-vod-row')].every(r=>!r.querySelector('.mv-team-tag'))"));
    await page.click('button[aria-label^="Rank:"]');
    assert.ok(await page.evaluate("[...document.querySelectorAll('.fa-picker-option')].filter(b=>b.querySelector('.fa-picker-count')?.textContent==='0').every(b=>b.disabled)"), 'zero-count options cannot be chosen');
    await page.click('.fa-picker-option[aria-pressed="true"]');
    await page.click('.mv-segment[aria-label="Perspective"] button:last-child');
    await page.wait("document.querySelectorAll('.mv-vod-row').length===1", 'reverse mid games only');
    await page.text('.mv-vod-list[aria-label="From Zed’s side"]', 'Zed player');
    assert.equal(await page.evaluate("!!document.querySelector('.mv-filters')"), true, 'under eight keeps the navigation filters');
    await page.click('.mv-matchup-scope button');
    await page.wait("document.querySelectorAll('.mv-vod-row').length===13", 'browse all clears filters and perspective');
    await page.click('button[aria-label="Champion: Ahri"]');
    await page.type('.fa-picker-search input', 'Zed');
    await page.click('.fa-picker-option', await page.evaluate("[...document.querySelectorAll('.fa-picker-option')].map(b=>b.textContent.trim()).find(t=>t.startsWith('Zed'))"));
    await page.wait("document.querySelectorAll('.mv-vod-row').length===2", 'searchable champion picker loads Zed');
    assert.equal(await page.evaluate("!!document.querySelector('.mv-filters')"), true);
    await nav('Dashboard', '.match-row');
    await nav('VODs', '.mv-vod-row');
    await page.visible('button[aria-label="Champion: All champions"]');
    await page.wait("document.querySelectorAll('.mv-vod-row').length===15", 'navigation resets to all champions');
    await page.click('button[aria-label="Champion: All champions"]');
    await page.type('.fa-picker-search input', 'Teemo');
    assert.ok(await page.evaluate("[...document.querySelectorAll('.fa-picker-option')].find(b=>b.textContent.startsWith('Teemo'))?.disabled"), 'champions with zero VODs are disabled');
    await page.click('button[aria-label="Champion: All champions"]');
    await nav('Matchups', '.fa-matchup-empty');
    await page.click('.fa-champion-card[data-champion-id="17"]');
    await page.click('.fa-show-all');
    await page.click('.fa-champion-card[data-champion-id="238"]');
    await page.text('.mv-entry', 'No VODs for this matchup');
    await page.click('.mv-entry button', 'Browse all Teemo VODs');
    await page.visible('button[aria-label="Champion: Teemo"]');
  });
  await check('VOD player controls and persistent mini player', async () => {
    await navigate(fullApp, '.match-row');
    await page.evaluate("localStorage.setItem('swapper.vodMomentsHidden','0')");
    await page.evaluate(`(() => {
      const invoke = window.__TAURI_INTERNALS__.invoke;
      // A bright headshot for one pro and a dark CDN-style silhouette for another; everyone else has none.
      const photos = { 'Fixture Pro 1': 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAgAAAAICAYAAADED76LAAAAFklEQVR4nGO8c+nEfwY8gAmf5PBRAACRSQOFCSL1+gAAAABJRU5ErkJggg==', 'Fixture Pro 2': 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAgAAAAICAYAAADED76LAAAAFUlEQVR4nGPk4eH7z4AHMOGTHD4KADM1ATVHev50AAAAAElFTkSuQmCC' };
      const playableVods = [{
        videoId:'A_b-cD12345',startSec:0,endSec:1200,title:'Ahri vs Zed · Challenger mid lane',player:'?',rankClaim:'Challenger',patch:'26.22',durationSec:1746,
        moments:[{atSec:184,label:'Ahri takes down Zed',kind:'kill',section:'early-lane'},{atSec:412,label:'Second fight',kind:'teamfight'}, {atSec:416,label:'Dragon secured',kind:'dragon'}, {atSec:710,label:'Ahri catches Zed at river',kind:'kill'}, {atSec:903,label:'Zed takes down Ahri',kind:'death'}, {atSec:1200,label:'Ahri wins the final fight',kind:'kill'}]
      }, {videoId:'B_b-cD12345',title:'Ahri vs Zed · KR Master mid lane',player:'Fixture Pro 3',rankClaim:'KR Master',patch:'26.22',durationSec:1500,result:'L',uploaded:'2026-01-05',moments:[]}].map(vod => ({...vod, enemy:238, lane:'mid'}));
      window.__TAURI_INTERNALS__.invoke = (cmd, args) => cmd === 'player_photo'
        ? (photos[args.player] ? Promise.resolve(photos[args.player]) : Promise.reject('That player photo is unavailable.'))
        : cmd === 'get_matchup_vods' ? Promise.resolve(playableVods)
        : cmd === 'get_all_vods' ? import('/src/full-app/fixtures/ahriChampionVods.ts').then(({ahriChampionVods}) => [...ahriChampionVods, ...playableVods].map(vod=>({...vod,champion:103})))
        : cmd === 'get_champion_vods' && args.championId === 103 ? import('/src/full-app/fixtures/ahriChampionVods.ts').then(({ahriChampionVods}) => [...ahriChampionVods, ...playableVods])
        : invoke(cmd, args);
    })()`);
    await nav('Matchups', '.fa-matchup-empty');
    await page.click('.fa-champion-card[data-champion-id="103"]');
    await page.click('.fa-champion-card[data-champion-id="238"]');
    await page.text('.mv-entry', '2 VODs for Ahri vs Zed');
    await page.click('.mv-entry button', 'Watch');
    await page.text('nav button[aria-current="page"]', 'VODs');
    await screenshot('vod-library-redesign', 1920, 1080);
    // Every Ahri VOD: the dev fixture holds 1,205 across 19 opponents and three lanes.
    await page.click('.mv-matchup-scope button');
    await page.wait("document.querySelectorAll('.mv-vod-row').length === 60", 'a 1,205-VOD library renders its first step');
    await page.text('.mv-match-count', '1,207 VODs');
    const row = name => `[...document.querySelectorAll('.mv-vod-row')].find(row => row.querySelector('.mv-who-line strong').textContent === ${JSON.stringify(name)})`;
    await page.wait(`!!${row('Fixture Pro 1')}?.querySelector('img.mv-row-photo')`, 'a pro with a real headshot shows it');
    assert.ok(await page.evaluate(`!${row('Fixture Pro 2')}.querySelector('img.mv-row-photo') && !!${row('Fixture Pro 2')}.querySelector('.mv-row-initials')`), 'a silhouette placeholder falls back to the rank crest');
    await screenshot('vod-library-photos', 1920, 1080);
    assert.ok(await page.evaluate("(() => { const row = document.querySelector('.mv-vod-row'), cells = [...row.children].map(c => c.getBoundingClientRect()); return Math.max(...cells.slice(1).map((c,i) => c.left - cells[i].right)) < row.getBoundingClientRect().width * .4; })()"), 'no dead column gap');
    assert.ok(await page.evaluate("[...document.querySelectorAll('.mv-thumb')].every(t => Math.abs(t.getBoundingClientRect().width / t.getBoundingClientRect().height - 16/9) < .02)"), 'thumbnail tiles keep 16:9');
    await page.wait("[...document.querySelectorAll('.mv-vod-row')].filter(r => r.dataset.vodId.startsWith('fixture-')).every(r => r.querySelector('.mv-thumb > .mv-thumb-splash'))", 'fixture thumbnails use champion splash art');
    assert.deepEqual(await page.evaluate(`(() => { const thumb = ${row('Fixture Pro 1')}.querySelector('.mv-thumb'); return { hidden: thumb.getAttribute('aria-hidden'), count: thumb.querySelector('.mv-thumb-moments').textContent, flag: !!thumb.querySelector('.mv-thumb-moments svg') }; })()`), { hidden: 'true', count: '4', flag: true }, 'moments count and flag sit inside the hidden thumbnail');
    assert.ok(await page.evaluate(`!${row('Fixture Pro 3')}.querySelector('.mv-thumb-moments')`), 'zero moments hides the thumbnail badge');
    assert.equal(await page.evaluate("document.querySelector('.mv-vod-head .is-end').textContent"), 'Result');
    assert.ok(await page.evaluate("[...document.querySelectorAll('.mv-cell-chips')].every(cell => !cell.querySelector('.mv-moments, .mv-chip'))"), 'the right column contains only the result');

    assert.ok(await page.evaluate("![...document.querySelectorAll('.mv-filters .fa-picker')].some(b => /Sort|patch first|Shortest|Longest/.test(b.getAttribute('aria-label')))"), 'no sort picker');
    assert.ok(await page.evaluate("[...document.querySelectorAll('.mv-vod-row')].every(row => row.getBoundingClientRect().height <= 64)"), 'one dense row per VOD');
    await page.evaluate("document.querySelector('.mv-vod-list').scrollTop = 1e6");
    await page.wait("document.querySelectorAll('.mv-vod-row').length === 120", 'scrolling renders the next step');
    await page.click('button[aria-label^="Opponent:"]');
    await page.visible('.fa-picker-option .champion-icon, .fa-picker-option .champion-icon-fallback');
    assert.ok(await page.evaluate("[...document.querySelectorAll('.fa-picker-option')].every(b => !b.querySelector('span').textContent.includes('·') && /^[\\d,]+$/.test(b.querySelector('.fa-picker-count').textContent))"), 'opponent counts sit apart from names');
    await page.evaluate("document.querySelector('.fa-picker-search input').focus()");
    await screenshot('vod-opponent-picker');
    await page.click('.fa-picker-option', await page.evaluate("[...document.querySelectorAll('.fa-picker-option')].map(b => b.textContent.trim()).find(t => t.startsWith('Yasuo'))"));
    await page.wait("/VODs/.test(document.querySelector('.mv-match-count').textContent) && [...document.querySelectorAll('.mv-vod-row')].every(row => row.getAttribute('aria-label').includes('vs Yasuo'))", 'opponent filter');
    await page.click('.mv-segment[aria-label="Players"] button:last-child');
    await page.wait("[...document.querySelectorAll('.mv-vod-row')].every(row => !row.querySelector('strong.is-unnamed'))", 'named players filter');
    await screenshot('vod-library-filtered');
    await page.cdp.send('Emulation.setDeviceMetricsOverride', { width: 1000, height: 700, deviceScaleFactor: 1, mobile: false });
    await page.settle();
    assert.ok(await page.evaluate("[...document.querySelectorAll('.mv-vod-row')].every(row => row.querySelector('.mv-who-line strong').checkVisibility() && row.querySelector('.mv-cell-length').checkVisibility() && row.scrollWidth <= row.clientWidth && row.getBoundingClientRect().height <= 64)"), 'small windows keep player, length and one dense row without overflow');
    await audit();
    await screenshot('vod-library-filtered-small', 1000, 700);
    await page.cdp.send('Emulation.setDeviceMetricsOverride', { width: 1280, height: 820, deviceScaleFactor: 1, mobile: false });
    await page.click('.mv-segment[aria-label="Players"] button:first-child');
    await page.click('button[aria-label^="Opponent:"]');
    await page.click('.fa-picker-option', await page.evaluate("[...document.querySelectorAll('.fa-picker-option')].map(b => b.textContent.trim()).find(t => t.startsWith('Zed'))"));
    await page.click('button[aria-label^="Patch:"]');
    await page.click('.fa-picker-option', await page.evaluate("[...document.querySelectorAll('.fa-picker-option')].map(b => b.textContent.trim()).find(t => t.startsWith('Patch 26.22'))"));
    await page.wait("document.querySelectorAll('.mv-vod-row').length === 2", 'back to the matchup list');
    await page.evaluate("window.__peer = null; window.__commands = []; addEventListener('message', e => { try { const d = JSON.parse(e.data); if (d.event === 'testViewport') window.__peer = d; if (d.event === 'testCommand') window.__commands.push(`${d.func}:${(d.args ?? []).join(',')}`); } catch {} })");
    await page.click('.mv-vod-row[data-vod-id="A_b-cD12345@0"]');
    await page.visible('.mv-embed iframe');
    assert.ok(await page.evaluate("new URL(document.querySelector('.mv-embed iframe').src).searchParams.get('controls') === '1' && !document.querySelector('.mv-quality-button, .mv-track, .mv-speed-button, .mv-time, .mv-control-row')"), "YouTube's own controls, with no second transport of ours");
    assert.ok(await page.evaluate("(() => { const h = document.querySelector('.mv-watch-header').getBoundingClientRect(); return h.bottom <= document.querySelector('.mv-embed').getBoundingClientRect().top; })()"), 'header stays above the picture');
    assert.ok(await page.evaluate("!document.querySelector('.mv-watch-header').textContent.includes('?')"), 'unknown pilot player is never displayed as a question mark');
    await page.text('.mv-header-matchup', 'Ahri');
    await page.text('.mv-header-matchup', 'vs');
    for (const [width, height] of [[1920, 1080], [1000, 700]]) {
      await screenshot('vod-header', width, height);
      assert.ok(await page.evaluate("(() => { const h = document.querySelector('.mv-watch-header'); return h.scrollWidth <= h.clientWidth && h.getBoundingClientRect().bottom <= document.querySelector('.mv-embed').getBoundingClientRect().top; })()"), 'header wraps without overflow above video');
    }
    // What YouTube sizes quality from: the iframe box and the viewport its document reports.
    const frameSize = "(() => { const f = document.querySelector('.mv-embed iframe'), r = f.getBoundingClientRect(); return { box: `${Math.round(r.width)}x${Math.round(r.height)}`, transform: getComputedStyle(f).transform, peer: window.__peer && `${window.__peer.width}x${window.__peer.height}@${window.__peer.dpr}x` }; })()";
    // Exactly 16:9, rail flush under the picture at its width, everything inside the window without nested scrolling.
    const layout = "(() => { const f = document.querySelector('.mv-embed iframe').getBoundingClientRect(), view = document.querySelector('.mv-player-view').getBoundingClientRect(), rail = document.querySelector('.mv-rail-row')?.getBoundingClientRect(); return { ratio: f.width / f.height, chromeAbove: Math.round(f.top - view.top), chromeBelow: Math.round(view.bottom - f.bottom), railFlush: !rail || (Math.abs(rail.top - (document.querySelector('.mv-video-player.is-fullscreen') ? f.top : f.bottom)) <= 1 && Math.abs(rail.left - f.left) <= 1 && Math.abs(rail.width - f.width) <= 1), inside: view.bottom <= innerHeight + 1 && document.querySelector('.mv-player-view').scrollHeight <= document.querySelector('.mv-player-view').clientHeight + 1 }; })()";
    const fitted = (state, label) => { assert.ok(Math.abs(state.ratio - 16 / 9) < 0.01, `${label}: picture is 16:9, got ${state.ratio}`); assert.ok(state.railFlush, `${label}: rail sits under the picture at its width`); };
    let windowedFrame, windowedLayout;
    for (const [width, height] of [[1920, 1080], [1655, 997], [1280, 820]]) {
      await page.cdp.send('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 1, mobile: false });
      await page.settle();
      const state = await page.evaluate(layout);
      fitted(state, `${width}×${height}`);
      assert.ok(state.inside, `video, rail and moments fit without nested scrolling at ${width}×${height}`);
      await screenshot('vod-fitted-player', width, height);
      if (width === 1920) { await page.wait("(() => { const r = document.querySelector('.mv-embed iframe').getBoundingClientRect(); return window.__peer && window.__peer.width === Math.round(r.width); })()", 'the video document has the windowed size'); windowedFrame = await page.evaluate(frameSize); windowedLayout = state; }
    }
    await page.evaluate("window.__vodFrame = document.querySelector('.mv-embed iframe'); window.__vodWindow = window.__vodFrame.contentWindow; window.__vodRoot = document.querySelector('.mv-video-player'); true");
    await page.wait("!!document.querySelector('.mv-tick:not(:disabled)')", 'the rail connects to the player');
    assert.ok(await page.evaluate("(() => { const f = document.querySelector('.mv-embed iframe').getBoundingClientRect(), b = document.querySelector('button[aria-label=\"Full screen\"]').getBoundingClientRect(); return Math.abs(b.right - f.right) <= 1 && b.top >= f.bottom - 1 && !document.querySelector('.mv-watch-header button[aria-label=\"Full screen\"]'); })()"), 'fullscreen sits bottom right under the picture, not in the header');
    const sent = (...commands) => page.wait(`window.__commands.slice(-${commands.length}).join(' ') === ${JSON.stringify(commands.join(' '))}`, `YouTube receives ${commands.join(' ')}`);
    await page.click('.mv-moment');
    await sent('seekTo:184,true', 'playVideo:');
    await page.wait("document.querySelector('.mv-tick.is-current')?.getAttribute('aria-label').includes('Ahri takes down Zed') && !!document.querySelector('.mv-moment[aria-current]')", 'rail and sidebar mark the current moment');
    await page.evaluate("document.querySelector('.mv-video-player').dispatchEvent(new KeyboardEvent('keydown',{key:']',bubbles:true}))");
    await sent('seekTo:412,true', 'playVideo:');
    await page.evaluate("document.querySelector('.mv-video-player').dispatchEvent(new KeyboardEvent('keydown',{key:'l',bubbles:true}))");
    await sent('seekTo:422,true');
    await page.evaluate("document.querySelector('.mv-video-player').dispatchEvent(new KeyboardEvent('keydown',{key:'k',bubbles:true}))");
    await sent('pauseVideo:');
    const tick = await page.evaluate("(() => { const r = document.querySelector('.mv-tick[aria-label*=\"Ahri takes down Zed\"]').getBoundingClientRect(); return { x: r.left + r.width / 2, y: r.top + r.height / 2 }; })()");
    await page.cdp.send('Input.dispatchMouseEvent', { type: 'mouseMoved', ...tick });
    await page.wait("[...document.querySelectorAll('.mv-tip')].some(tip => tip.checkVisibility() && tip.querySelector('.mv-portraits .champion-icon') && tip.textContent.includes('3:04'))", 'hovering a tick shows its moment with champion portraits');
    await screenshot('vod-player-polished');
    await page.text('.mv-tick-count', '2');
    await page.click('.mv-tick[aria-label*="Second fight"]');
    await sent('seekTo:412,true', 'playVideo:');
    await page.text('.mv-player-name', 'Challenger replay');
    const noOverlap = "(() => { const f = document.querySelector('.mv-embed iframe').getBoundingClientRect(); return [...document.querySelectorAll('.mv-video-player button, .mv-tip, .mv-notice, .mv-game-over, .mv-side, .mv-mini-actions')].filter(e => e.checkVisibility()).every(e => { const r = e.getBoundingClientRect(); return r.bottom <= f.top || r.top >= f.bottom || r.right <= f.left || r.left >= f.right; }); })()";
    assert.ok(await page.evaluate(noOverlap), 'inline tools stay outside the iframe');
    await page.cdp.send('Input.dispatchMouseEvent', { type: 'mouseMoved', ...await page.evaluate("(() => { const r = document.querySelector('.mv-tick').getBoundingClientRect(); return {x:r.left+r.width/2,y:r.top+r.height/2}; })()") });
    assert.ok(await page.evaluate(noOverlap), 'moment tooltips stay below the iframe');
    await page.click('button[aria-label="Full screen"]');
    await page.visible('.mv-video-player.is-fullscreen');
    await page.cdp.send('Emulation.setDeviceMetricsOverride', { width: 1920, height: 1080, deviceScaleFactor: 1, mobile: false });
    await page.settle();
    const videoOnly = "(() => { const root = document.querySelector('.mv-video-player'), f = root.querySelector('iframe').getBoundingClientRect(); return f.top === 0 && f.left === 0 && Math.abs(f.width - innerWidth) <= 1 && Math.abs(f.height - innerHeight) <= 1 && !root.querySelector('button, header, .mv-side, .mv-rail-row, .mv-notice, .mv-game-over, .mv-tip') && [...root.querySelectorAll('*')].every(e => e.contains(root.querySelector('iframe'))); })()";
    assert.ok(await page.evaluate(videoOnly), 'fullscreen contains only the video, with no overlays');
    await screenshot('vod-fullscreen-redesign', 1920, 1080);
    await page.evaluate("window.dispatchEvent(new MessageEvent('message', { source: document.querySelector('.mv-embed iframe').contentWindow, origin: 'https://www.youtube-nocookie.com', data: JSON.stringify({event:'infoDelivery',info:{currentTime:1201,playerState:1}}) }))");
    await page.visible('.mv-game-over');
    assert.ok(await page.evaluate("!document.querySelector('.mv-video-player.is-fullscreen') && document.querySelector('.mv-game-over').getBoundingClientRect().top >= document.querySelector('.mv-embed iframe').getBoundingClientRect().bottom"), 'segment end exits fullscreen and puts actions below the picture');
    await screenshot('vod-segment-end');
    await page.click('.mv-game-over button', 'Keep watching');
    await page.cdp.send('Emulation.setDeviceMetricsOverride', { width: 1280, height: 820, deviceScaleFactor: 1, mobile: false });
    await page.evaluate("window.__nativeFullscreen = window.__vodRoot.requestFullscreen; window.__vodRoot.requestFullscreen = () => Promise.reject(new Error('test refused fullscreen'))");
    await page.click('button[aria-label="Full screen"]');
    await page.wait("document.querySelector('.mv-video-player').matches(':popover-open')", 'Tauri fallback uses top layer');
    assert.ok(await page.evaluate("(() => { const r = document.querySelector('.mv-video-player').getBoundingClientRect(); return r.top === 0 && r.left === 0 && r.bottom === innerHeight && r.right === innerWidth && document.querySelector('.mv-embed iframe') === window.__vodFrame; })()"), 'fallback fills viewport without remounting the video');
    await screenshot('vod-fullscreen-fallback');
    await page.evaluate("document.querySelector('.mv-video-player').dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true}))");
    await page.wait("!document.querySelector('.mv-video-player').matches(':popover-open') && !document.querySelector('.mv-video-player.is-fullscreen')", 'Escape exits fallback fullscreen');
    await page.evaluate("window.__vodRoot.requestFullscreen = window.__nativeFullscreen");
    // Some WebViews cannot provide either top-layer API. The CSS fallback must
    // still escape shell containment, hide its title bar and fill the viewport.
    await page.evaluate("window.__nativePopover = window.__vodRoot.showPopover; window.__vodRoot.showPopover = undefined; window.__vodRoot.requestFullscreen = () => Promise.reject(new Error('test CSS-only fullscreen'))");
    await page.click('button[aria-label="Full screen"]');
    await page.visible('.mv-video-player.is-fullscreen');
    await page.settle();
    assert.ok(await page.evaluate("(() => { const r = document.querySelector('.mv-video-player').getBoundingClientRect(); return r.top === 0 && r.left === 0 && r.bottom === innerHeight && r.right === innerWidth && !document.querySelector('.title-bar').checkVisibility() && document.querySelector('.mv-video-player').contains(document.elementFromPoint(10, 10)); })()"), 'CSS-only fullscreen covers the shell and every viewport edge');
    await screenshot('vod-fullscreen-css-fallback');
    await page.evaluate("document.querySelector('.mv-video-player').dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true}))");
    await page.visible('.title-bar');
    await page.evaluate("window.__vodRoot.requestFullscreen = window.__nativeFullscreen; window.__vodRoot.showPopover = window.__nativePopover");
    await page.cdp.send('Emulation.setDeviceMetricsOverride', { width: 1000, height: 700, deviceScaleFactor: 1, mobile: false });
    await page.settle();
    await audit();
    await screenshot('vod-player-small', 1000, 700);
    fitted(await page.evaluate(layout), '1000×700');
    assert.ok(await page.evaluate("document.querySelector('.mv-player-view').scrollWidth <= document.querySelector('.mv-player-view').clientWidth"), 'narrow player has no horizontal overflow');
    assert.ok(await page.evaluate("document.querySelector('.mv-rail-row').getBoundingClientRect().bottom <= innerHeight"), 'small-window moment rail remains visible');
    await page.click('.mv-moment[aria-label*="final fight"]');
    await screenshot('vod-story-small', 1000, 700);
    await page.cdp.send('Emulation.setDeviceMetricsOverride', { width: 1280, height: 820, deviceScaleFactor: 1, mobile: false });
    await page.click('.mv-section-link');
    await page.visible('.mg-guide-view');
    await page.visible('.is-mini iframe');
    assert.ok(await page.evaluate(noOverlap), 'mini controls stay outside the iframe');
    assert.equal(await page.evaluate("getComputedStyle(document.querySelector('.is-mini iframe')).pointerEvents"), 'none');
    for (const [label, command] of [['Pause video','pauseVideo'],['Play video','playVideo'],['Mute video','mute'],['Unmute video','unMute']]) {
      await page.evaluate("window.__commands = []");
      await page.click(`.mv-mini-actions button[aria-label="${label}"]`);
      await page.wait(`window.__commands.includes(${JSON.stringify(command + ':')})`, `mini sends ${command}`);
    }
    const miniBox = () => page.evaluate("(() => {const r=document.querySelector('.is-mini .mv-video-player').getBoundingClientRect();return {x:r.x,y:r.y,width:r.width,height:r.height};})()");
    const originalMini = await miniBox();
    const dragMini = async (x,y,dx,dy) => {
      await page.cdp.send('Input.dispatchMouseEvent',{type:'mousePressed',x,y,button:'left',clickCount:1});
      await page.cdp.send('Input.dispatchMouseEvent',{type:'mouseMoved',x:x+dx,y:y+dy,button:'left',buttons:1});
      await page.cdp.send('Input.dispatchMouseEvent',{type:'mouseReleased',x:x+dx,y:y+dy,button:'left',clickCount:1});
      await page.settle();
    };
    await dragMini(originalMini.x+35,originalMini.y+25,-100,-80);
    const movedMini = await miniBox();
    assert.ok(movedMini.x < originalMini.x-50 && movedMini.y < originalMini.y-50, 'drag moves mini');
    for (const corner of ['nw','ne','sw','se']) {
      const before = await miniBox();
      const west = corner.includes('w'), north = corner.includes('n');
      const handle = await page.evaluate(`(() => {const h=document.querySelector('.mv-mini-resize[data-corner="${corner}"]'),r=h.getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2,cursor:getComputedStyle(h).cursor};})()`);
      assert.equal(handle.cursor, ['nw','se'].includes(corner) ? 'nwse-resize' : 'nesw-resize');
      assert.ok(await page.evaluate(`(() => {const h=document.querySelector('.mv-mini-resize[data-corner="${corner}"]').getBoundingClientRect(),v=document.querySelector('.is-mini iframe').getBoundingClientRect();return h.right<=v.left || h.left>=v.right || h.bottom<=v.top || h.top>=v.bottom;})()`), `${corner} handle stays outside picture`);
      await dragMini(handle.x,handle.y,west ? -40 : 40,north ? -22.5 : 22.5);
      const after = await miniBox();
      assert.ok(after.width > before.width+30, `${corner} resizes mini`);
      assert.ok(Math.abs((west ? after.x+after.width : after.x)-(west ? before.x+before.width : before.x))<1, `${corner} anchors opposite x`);
      assert.ok(Math.abs((north ? after.y+after.height : after.y)-(north ? before.y+before.height : before.y))<1, `${corner} anchors opposite y`);
      assert.ok(await page.evaluate("(() => {const r=document.querySelector('.is-mini iframe').getBoundingClientRect();return Math.abs(r.width/r.height-16/9)<.01;})()"), `${corner} preserves aspect`);
      if (corner !== 'se') {
        const handleAfter = await page.evaluate(`(() => {const r=document.querySelector('.mv-mini-resize[data-corner="${corner}"]').getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2};})()`);
        await dragMini(handleAfter.x,handleAfter.y,west ? 40 : -40,north ? 22.5 : -22.5);
      }
    }
    const resizedMini = await miniBox();
    assert.ok(await page.evaluate("JSON.parse(localStorage.getItem('swapper.vodMiniPlacement')).width > 360"), 'size and position are saved');
    for (const type of ['mousePressed','mouseReleased']) await page.cdp.send('Input.dispatchMouseEvent',{type,x:resizedMini.x+35,y:resizedMini.y+25,button:'left',clickCount:2});
    await page.settle();
    const resetMini = await miniBox();
    assert.ok(Math.abs(resetMini.x-originalMini.x)<2 && Math.abs(resetMini.width-originalMini.width)<2, 'header double click resets placement');

    assert.ok(await page.evaluate(`(() => {const b=document.querySelector('button[aria-label="Next guide section"]');const r=b.getBoundingClientRect();return document.elementFromPoint(r.x+r.width/2,r.y+r.height/2)?.closest('button')===b;})()`), 'mini player leaves Next reachable');
    assert.ok(await page.evaluate("document.querySelector('.mv-embed iframe') === window.__vodFrame && window.__vodFrame.contentWindow === window.__vodWindow"), 'guide navigation must retain the iframe and its browsing context');
    assert.ok(await page.evaluate("(() => { const f = document.querySelector('.is-mini iframe').getBoundingClientRect(); return Math.abs(f.width / f.height - 16 / 9) < 0.01 && !document.querySelector('.is-mini :is(.mv-rail-row, .mv-side, .mv-watch-header)'); })()"), 'the mini player is a 16:9 video without rail, sidebar or header');
    await page.evaluate("window.__TAURI_INTERNALS__.invoke('plugin:event|emit', {event:'champ_select',payload:{phase:'ChampSelect'}})");
    await page.settle();
    assert.ok(await page.evaluate("window.__vodFrame.isConnected"), 'entering draft must not stop the VOD');
    await page.evaluate("window.__TAURI_INTERNALS__.invoke('plugin:event|emit', {event:'champ_select',payload:{phase:'None'}})");
    await page.evaluate("window.previewGamePhase('InProgress')");
    await page.wait("Array.from(document.querySelectorAll('nav button')).some(button => button.getAttribute('aria-label') === 'Draft' && button.querySelector('.nav-badge.live'))", 'combined draft route stays available during playback');
    assert.ok(await page.evaluate("window.__vodFrame.isConnected"), 'starting a game must not stop the VOD');
    await page.evaluate("window.previewGamePhase('None')");
    await screenshot('vod-mini-player');
    await page.click('.is-mini .mv-embed');
    await page.visible('.mv-player-view:not(.is-mini) iframe');
    assert.ok(await page.evaluate("document.querySelector('.mv-embed iframe') === window.__vodFrame"));
    await nav('Settings', '.full-app-embed');
    assert.ok(await page.evaluate("document.querySelector('.is-mini iframe') === window.__vodFrame"), 'settings retains the same player');
    await page.click('button[aria-label="Close mini player"]');
    await page.wait("!document.querySelector('.mv-embed iframe')", 'mini player closed');
    await nav('VODs', '.mv-vod-row');
    await page.visible('.mv-vod-row');
    // Most VODs have no key moments: then the picture takes the whole width, with only the fullscreen bar below it.
    await page.cdp.send('Emulation.setDeviceMetricsOverride', { width: 1920, height: 1080, deviceScaleFactor: 1, mobile: false });
    await page.click('.mv-vod-row[data-vod-id="B_b-cD12345@0"]');
    await page.visible('.mv-embed iframe');
    await page.settle();
    const plain = await page.evaluate("(() => { const f = document.querySelector('.mv-embed iframe').getBoundingClientRect(), stage = document.querySelector('.mv-stage').getBoundingClientRect(); return { none: !document.querySelector('.mv-side, .mv-rail, .mv-chapters') && document.querySelectorAll('.mv-rail-row button').length === 1, filled: Math.abs(f.width - stage.width) <= 1 || Math.abs(f.height + 32 - stage.height) <= 1, facts: document.querySelector('.mv-facts').textContent }; })()");
    assert.ok(plain.none && plain.filled, `no moments: no sidebar or rail, only fullscreen below, and the picture fills its space (${JSON.stringify(plain)})`);
    assert.ok(/Patch 26.22/.test(plain.facts) && /Jan 5, 2026/.test(plain.facts), `header facts: patch and upload date (${plain.facts})`);
    await page.text('.mv-header-rank', 'Master');
    fitted(await page.evaluate(layout), 'no moments');
    const plainFrame = await page.evaluate(frameSize);
    await screenshot('vod-player-no-moments', 1920, 1080);
    await page.click('button[aria-label="Full screen"]');
    await page.visible('.mv-video-player.is-fullscreen');
    await page.settle();
    assert.ok(await page.evaluate(videoOnly), 'fullscreen without moments is video only');
    console.log(`  no-moment VOD at 1920x1080: windowed ${JSON.stringify(plainFrame)} | fullscreen ${JSON.stringify(await page.evaluate(frameSize))}`);
    await page.evaluate("document.querySelector('.mv-video-player').dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true}))");
    await page.wait("!document.fullscreenElement && !document.querySelector('.mv-video-player.is-fullscreen')", 'Escape leaves fullscreen');
    await page.cdp.send('Emulation.setDeviceMetricsOverride', { width: 1280, height: 820, deviceScaleFactor: 1, mobile: false });
    await page.click('.mv-back');
    await page.visible('.mv-vod-row');
    await page.evaluate("window.previewGamePhase('InProgress')");
    await page.text('nav button[aria-current="page"]', 'Draft');
    // The lazy Live Game page must commit before the game ends; otherwise the matchup page never unmounts and the next check inherits it.
    await page.visible(':is(.lg, .lg-empty)');
    await page.evaluate("window.previewGamePhase('None')");
  });
  await check("Matchups champion grid fills the page at desktop widths", async () => {
    await navigate(fullApp, ".match-row");
    await nav("Matchups", ".fa-matchup-empty");
    await championGridLayout();
  });
  await check("Matchups Auto ignores a recent off-role pick", async () => {
    await navigate(fullApp + "?matchup-roles", ".match-row");
    await nav("Matchups", ".fa-matchup-empty");
    await page.click(".fa-quick-pick");
    await page.text('.role-tabs button[aria-pressed="true"]', "Auto");
    await page.text(".fa-matchup-picks .cs-slot.is-you", "LeBlanc");
    await page.text(".fa-matchup-context", "Mid");
    assert.equal(await page.evaluate("document.querySelector('.selected-champion')"), null);
    assert.equal(await page.evaluate("document.querySelector('.fa-champion-card[data-champion-id=\"222\"]')"), null);
  });
  await check("Matchups opponent selection shows your champion versus an empty role slot", async () => {
    await page.visible(".fa-matchup-picks .cs-slot.is-you");
    await page.text(".fa-matchup-picks .cs-slot.is-you", "LeBlanc");
    await page.text(".fa-matchup-picks .cs-slot.is-empty", "Mid");
    await page.visible(".fa-matchup-picks .cs-slot.is-empty .cs-slot-placeholder");
    await page.text(".fa-matchup-vs", "VS");
    assert.equal(await page.evaluate("document.querySelector('.fa-champion-grid [data-champion-id=\"7\"]')"), null);
    await championGridLayout();
  });
  await check("Matchups hero omits step labels and offers a repick button", async () => {
    assert.equal(await page.evaluate("document.querySelector('.fa-matchup-empty').textContent.includes('Step 2 of 2')"), false);
    await page.visible('button[aria-label="Change your champion"]');
    await searchPillFocus();
    const button = 'button[aria-label="Change your champion"]';
    await cdp.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: 0, y: 0 });
    await page.wait("getComputedStyle(document.querySelector('.fa-repick')).opacity === '0'", "repick overlay hidden at rest");
    const position = await page.evaluate("(() => { const r = document.querySelector('.fa-repick').getBoundingClientRect(); return {x: r.left + r.width / 2, y: r.top + r.height / 2}; })()");
    await cdp.send("Input.dispatchMouseEvent", { type: "mouseMoved", ...position });
    await page.wait("getComputedStyle(document.querySelector('.fa-repick')).opacity === '1'", "repick overlay on hover");
    await page.click(button);
    await page.text(".fa-matchup-prompt h2", "Pick your champion");
    assert.equal(await page.evaluate("document.querySelector('.fa-matchup-empty').textContent.includes('Step 1 of 2')"), false);
    await page.click(".fa-quick-pick");
    await page.visible(button);
    await cdp.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: 0, y: 0 });
    await page.evaluate("document.querySelector('.search-pill input').focus()");
    await cdp.send("Input.dispatchKeyEvent", { type: "keyDown", key: "Tab", code: "Tab", windowsVirtualKeyCode: 9 });
    await cdp.send("Input.dispatchKeyEvent", { type: "keyUp", key: "Tab", code: "Tab", windowsVirtualKeyCode: 9 });
    await page.wait("document.activeElement.matches('.fa-repick:focus-visible') && getComputedStyle(document.activeElement).opacity === '1'", "repick overlay on keyboard focus");
    await cdp.send("Input.dispatchKeyEvent", { type: "keyDown", key: "Enter", code: "Enter", text: "\r", windowsVirtualKeyCode: 13 });
    await cdp.send("Input.dispatchKeyEvent", { type: "keyUp", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13 });
    await page.text(".fa-matchup-prompt h2", "Pick your champion");
    await page.click(".fa-quick-pick");
    await page.text(".fa-matchup-picks .cs-slot.is-you", "LeBlanc");
  });
  await check("Matchups grid suggests opponents, shows all and opens on the second pick", async () => {
    await page.click(".fa-show-all");
    await page.visible('.fa-champion-card[data-champion-id="222"]');
    await page.visible('.fa-show-all[aria-pressed="true"]');
    assert.equal(await page.evaluate("document.querySelector('.fa-champion-grid [data-champion-id=\"7\"]')"), null);
    await screenshot("matchups-opponents-all");
    await screenshot("matchups-opponents-all", 1920, 1080);
    await page.cdp.send("Emulation.setDeviceMetricsOverride", { width: 1280, height: 820, deviceScaleFactor: 1, mobile: false });
    await page.click(".fa-show-all");
    assert.equal(await page.evaluate("document.querySelector('.fa-champion-card[data-champion-id=\"222\"]')"), null);
    await page.click('.fa-champion-card[data-champion-id="103"]');
    await page.text('[aria-label="Matchup"]', "LeBlanc");
    await page.text('[aria-label="Matchup"]', "Ahri");
    await page.text('[aria-label="Matchup"]', "Mid");
    await page.text('[aria-label="Matchup"]', "47.6");
    assert.equal(await page.evaluate("document.querySelector('.fa-champion-grid')"), null);
    assert.equal(await page.evaluate("document.activeElement.textContent"), "← Back to opponents");
    await cdp.send("Input.dispatchKeyEvent", { type: "keyDown", key: "Escape", code: "Escape", windowsVirtualKeyCode: 27 });
    await cdp.send("Input.dispatchKeyEvent", { type: "keyUp", key: "Escape", code: "Escape", windowsVirtualKeyCode: 27 });
    await page.text(".fa-matchup-picks .cs-slot.is-you", "LeBlanc");
    assert.equal(await page.evaluate("document.activeElement.getAttribute('aria-label')"), "Search champion");
    await page.click('.fa-champion-card[data-champion-id="103"]');
    await page.text('[aria-label="Matchup"]', "Ahri");
    // The existing detail pickers and matchup strip still change the opponent.
    await page.click('button[aria-label^="Opponent:"]');
    await page.click(".fa-picker-option", "Darius");
    await page.text('[aria-label="Matchup"]', "Darius");
    await page.click('.fa-matchup-card[data-tooltip^="Jax:"]');
    await page.text('[aria-label="Matchup"]', "Jax");
    await searchPillFocus('.fa-strip-search');
    await page.evaluate("document.activeElement.blur()");
  });
  await check("Matchups can undo picks, search and choose with the keyboard", async () => {
    await page.click(".back-to-list", "← Back to opponents");
    await page.text(".fa-matchup-picks .cs-slot.is-you", "LeBlanc");
    await page.type('input[aria-label="Search champion"]', "missing-opponent");
    await page.text(".fa-matchup-empty", "No opponent matches");
    const escape = async () => {
      await cdp.send("Input.dispatchKeyEvent", { type: "keyDown", key: "Escape", code: "Escape", windowsVirtualKeyCode: 27 });
      await cdp.send("Input.dispatchKeyEvent", { type: "keyUp", key: "Escape", code: "Escape", windowsVirtualKeyCode: 27 });
    };
    await escape();
    await page.visible('.fa-champion-card[data-champion-id="103"]');
    await escape();
    await page.text(".fa-matchup-prompt h2", "Pick your champion");
    await page.click('.role-tabs button', "Top");
    await page.evaluate("document.querySelector('.fa-champion-card[data-champion-id=\"7\"]').focus()");
    await cdp.send("Input.dispatchKeyEvent", { type: "keyDown", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13, text: "\r" });
    await cdp.send("Input.dispatchKeyEvent", { type: "keyUp", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13 });
    await page.text(".fa-matchup-picks .cs-slot.is-empty", "Top");
    await page.click('button[aria-label="Change your champion"]');
    await page.text(".fa-matchup-prompt h2", "Pick your champion");
    await page.click('.role-tabs button', "Auto");
    await page.click(".fa-quick-pick");
    await page.text(".fa-matchup-picks .cs-slot.is-empty", "Mid");
    await page.click('button[aria-label="Change your champion"]');
    await page.text(".fa-matchup-prompt h2", "Pick your champion");
    await navigate(fullApp, ".match-row");
  });
  await check("Sidebar Patch notes", async () => {
    await page.text('nav[aria-label="Swapper pages"] .nav-badge.new', "NEW!");
    await page.click('nav[aria-label="Swapper pages"] button', "Patch notesNEW!");
    await page.text('nav[aria-label="Swapper pages"] [aria-current="page"]', "Patch notes");
    await page.visible(".pn-entry");
    assert.equal(await page.evaluate("document.querySelector('.sidebar .nav-badge.new')"), null);
    await page.text(".pn-entry", "Azir");
    await page.text(".pn-entry", "50/Level");
    await page.text(".pn-patch-line h2", "Patch 26.21");
    await page.text(".pn-summary", "2 buffs");
    await page.text(".pn-summary", "2 nerfs");
    // The overview groups champions by verdict and jumps to a card.
    await page.text(".pov-group.is-buff", "Azir");
    await page.click(".pov-group.is-buff .pov-champion");
    await page.wait("document.activeElement?.classList.contains('pn-entry') && document.querySelector('.dashboard-main').scrollTop > 0", "overview jump focuses and scrolls to the card");
    await page.wait("(() => { const m = document.querySelector('.dashboard-main'); const t = m.scrollTop; return new Promise(r => setTimeout(() => r(m.scrollTop === t), 300)); })()", "overview scroll settles");
    await page.evaluate("document.activeElement.blur(); document.querySelector('.dashboard-main').scrollTop = 0");
    assert.equal(await page.evaluate("/ARAM|Mayhem|Arena|cosmetic/.test(document.querySelector('.pn-page').textContent)"), false);
    assert.equal(await page.evaluate("document.querySelector('.pn-bugfixes').open"), false);
    assert.equal(await page.evaluate("document.querySelectorAll('.pn-checkpoint').length"), 8);
    await page.text('.pn-entry[aria-label="Caitlyn"] h4', "Q - Piltover Peacemaker");
    await page.text('.pn-entry[aria-label="Caitlyn"] h4', "Passive - Headshot");
    await page.text('.pn-entry[aria-label="Lee Sin"] h4', "R - Dragon's Rage");
    await page.text('.pn-entry[aria-label="Lee Sin"] h4', "E - Tempest");
    await page.wait("Array.from(document.querySelectorAll('.pn-ability-icon')).length === 3 && Array.from(document.querySelectorAll('.pn-ability-icon')).every(img => img.complete && img.naturalWidth > 0)");
    assert.equal(await page.evaluate("Array.from(document.querySelectorAll('.pn-ability-icon')).every(img => img.getBoundingClientRect().left < img.nextElementSibling.getBoundingClientRect().left)"), true, "ability icons appear to the left of the key and name");
    const kick = '.pn-entry[aria-label="Lee Sin"] .pn-group:first-child .pn-after';
    assert.deepEqual(await page.evaluate(`Array.from(document.querySelectorAll('${kick} .is-nerf')).map(el => el.textContent)`), ["150", "350", "550"]);
    assert.equal(await page.evaluate(`document.querySelector('${kick}').querySelector('.is-neutral').textContent`), " / ");
    assert.equal(await page.evaluate(`Array.from(document.querySelectorAll('${kick} .is-neutral')).some(el => el.textContent === '+200')`), true);
    await page.text('.pn-entry[aria-label="Lee Sin"] .pn-tag', "Nerf");
    assert.equal(await page.evaluate("document.querySelector('.pn-entry[aria-label=\"Lee Sin\"] .pn-group:last-child .pn-after .is-nerf').textContent"), "15");
    assert.deepEqual(await page.evaluate("Array.from(document.querySelectorAll('.pn-entry[aria-label=\"Lethal Tempo\"] .pn-after :is(.is-buff, .is-nerf)')).map(el => [el.textContent, el.classList.contains('is-buff') ? 'buff' : 'nerf'])"), [["6", "buff"], ["8", "nerf"]]);
    await page.text('.pn-entry[aria-label="Lethal Tempo"] .pn-tag', "Nerf");
    assert.equal(await page.evaluate("document.querySelector('.pn-entry[aria-label=\"Hubris\"] .pn-after :is(.is-buff, .is-nerf)') === null"), true, "mismatched value shapes stay neutral");
    assert.equal(await page.evaluate("document.querySelector('.pn-entry[aria-label=\"New Preview Relic\"] .pn-entry-letter').textContent"), "N", "unmatched item keeps its letter tile");
    await page.evaluate("document.querySelector('.pn-entry[aria-label=\"Dawncore\"] .item-icon')?.scrollIntoView({ block: 'center', behavior: 'instant' })");
    await page.wait("(() => { const icon = document.querySelector('.pn-entry[aria-label=\"Dawncore\"] .item-icon'); return icon?.complete && icon.naturalWidth > 0; })()", "resolved item uses the existing ItemIcon");
    await page.evaluate("document.querySelector('.dashboard-main').scrollTop = 0");
    assert.equal(await page.evaluate("document.querySelector('.pn-bugfixes').open"), false, "bugfixes are initially collapsed");
    assert.equal(await page.evaluate(`getComputedStyle(document.querySelector('${kick} .is-nerf')).color === getComputedStyle(document.querySelector('.pn-summary .is-nerf')).color`), true, "numeric nerfs use the loss token");
    assert.equal(await page.evaluate("getComputedStyle(document.querySelector('.pn-entry[aria-label=\"Azir\"] .pn-after .is-buff')).color === getComputedStyle(document.querySelector('.pn-summary .is-buff')).color"), true, "numeric buffs use the win token");
    await page.cdp.send("Emulation.setDeviceMetricsOverride", { width: 1920, height: 1080, deviceScaleFactor: 1, mobile: false });
    await page.settle();
    assert.equal(await page.evaluate("(() => { const main = document.querySelector('.dashboard-main'); const card = document.querySelector('.pn-entry'); return Math.abs(card.getBoundingClientRect().left - main.getBoundingClientRect().left - main.clientLeft - parseFloat(getComputedStyle(main).paddingLeft)) < 1; })()"), true, "wide cards start at the canvas content edge");
    assert.equal(await page.evaluate("parseFloat(getComputedStyle(document.querySelector('.pn-line')).fontSize) >= 16"), true);
    const checkComparisonLayout = async () => {
      const spacing = await page.evaluate(`(() => {
        const row = document.querySelector('.pn-entry[aria-label="Azir"] .pn-line');
        const label = row.querySelector('.pn-line-label');
        const labelRect = label.getBoundingClientRect();
        const before = row.querySelector('.pn-before').getBoundingClientRect();
        const after = row.querySelector('.pn-after').getBoundingClientRect();
        const arrow = row.querySelector('.pn-arrow');
        const range = document.createRange();
        range.setStart(arrow.firstChild, 0); range.setEnd(arrow.firstChild, 1);
        const glyph = range.getBoundingClientRect();
        const blocks = document.querySelectorAll('.pn-entry[aria-label="Jungle"] .pn-line');
        return { labelAbove: before.top >= labelRect.bottom, labelLeft: Math.abs(before.left - labelRect.left),
          smallerLabel: parseFloat(getComputedStyle(label).fontSize) < parseFloat(getComputedStyle(row).fontSize),
          brighterAfter: getComputedStyle(row.querySelector('.pn-after')).color !== getComputedStyle(row.querySelector('.pn-before')).color,
          blockGap: blocks[1].getBoundingClientRect().top - blocks[0].getBoundingClientRect().bottom,
          beforeGap: glyph.left - before.right, afterGap: after.left - glyph.right,
          arrow: arrow.textContent, sameLine: Math.abs(before.top - after.top) < 2 };
      })()`);
      assert.equal(spacing.arrow, "→");
      assert.equal(spacing.sameLine, true);
      assert.equal(spacing.labelAbove, true, "the stat label sits above its values");
      assert.ok(spacing.labelLeft < 1, "label and values begin at the same left edge");
      assert.equal(spacing.smallerLabel, true);
      assert.equal(spacing.brighterAfter, true);
      assert.ok(spacing.blockGap >= 18, "each change has space before the next block");
      for (const gap of [spacing.beforeGap, spacing.afterGap]) {
        assert.ok(gap >= 10 && gap <= 12, `comparison arrow has a comfortable local gap: ${JSON.stringify(spacing)}`);
      }
    };
    await checkComparisonLayout();
    await page.cdp.send("Emulation.setDeviceMetricsOverride", { width: 760, height: 820, deviceScaleFactor: 1, mobile: false });
    await page.settle();
    await checkComparisonLayout();
    assert.equal(await page.evaluate("document.querySelector('.pn-entry[aria-label=\"Lee Sin\"] .pn-change').getClientRects().length > 1"), true, "long comparisons wrap naturally");
    assert.equal(await page.evaluate("Array.from(document.querySelectorAll('.pn-line')).every(line => line.scrollWidth <= line.clientWidth + 1)"), true, "wrapped comparisons stay inside their cards");
    await screenshot("patch-notes-comparison-small", 760, 820);
    await page.cdp.send("Emulation.setDeviceMetricsOverride", { width: 1280, height: 820, deviceScaleFactor: 1, mobile: false });
    await page.settle();
    await screenshot("patch-notes-top");
    const marker = await page.evaluate("document.querySelector('.pn-rail-marker').style.transform");
    await page.click('.pn-checkpoint[aria-label="Read Caitlyn changes"]');
    await page.wait("document.querySelector('.pn-checkpoint[aria-current=location]').getAttribute('aria-controls') === 'pn-champions-1'");
    await page.wait("(() => { const main = document.querySelector('.dashboard-main'); const ratio = (document.getElementById('pn-champions-1').getBoundingClientRect().top - main.getBoundingClientRect().top - main.clientTop) / main.clientHeight; return ratio >= .25 && ratio <= .3; })()");
    assert.notEqual(await page.evaluate("document.querySelector('.pn-rail-marker').style.transform"), marker);
    assert.equal(await page.evaluate("window.scrollY"), 0, "the app container owns scrolling");
    const atCaitlyn = await page.evaluate("document.querySelector('.pn-rail-marker').style.transform");
    await page.evaluate("document.querySelector('.dashboard-main').scrollTop += 75");
    await page.wait(`document.querySelector('.pn-rail-marker').style.transform !== ${JSON.stringify(atCaitlyn)}`);
    await page.evaluate("(() => { const main = document.querySelector('.dashboard-main'); const card = document.getElementById('pn-champions-2'); main.scrollTo({ top: main.scrollTop + card.getBoundingClientRect().top - main.getBoundingClientRect().top - main.clientTop - main.clientHeight * .42, behavior: 'instant' }); })()");
    await page.wait("document.querySelector('.pn-checkpoint[aria-current=location]').getAttribute('aria-controls') === 'pn-champions-1'");
    await page.evaluate("document.querySelector('.dashboard-main').scrollTop += document.querySelector('.dashboard-main').clientHeight * .04");
    await page.wait("document.querySelector('.pn-checkpoint[aria-current=location]').getAttribute('aria-controls') === 'pn-champions-2'", "the next card becomes active as it crosses 40% of the viewport");
    await page.cdp.send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: "reduce" }] });
    await page.evaluate("document.querySelector('.pn-checkpoint[aria-label=\"Read Jinx changes\"]').focus()");
    await page.cdp.send("Input.dispatchKeyEvent", { type: "keyDown", key: "Enter", code: "Enter", text: "\r", unmodifiedText: "\r", windowsVirtualKeyCode: 13 });
    await page.cdp.send("Input.dispatchKeyEvent", { type: "keyUp", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13 });
    await page.wait("document.activeElement.id === 'pn-champions-2' && document.querySelector('.pn-checkpoint[aria-current=location]').getAttribute('aria-controls') === 'pn-champions-2'");
    await page.wait("(() => { const main = document.querySelector('.dashboard-main'); const ratio = (document.activeElement.getBoundingClientRect().top - main.getBoundingClientRect().top - main.clientTop) / main.clientHeight; return ratio >= .25 && ratio <= .3; })()", "keyboard jumps place the focused card below the top quarter");
    await page.click('.pn-checkpoint[aria-label="Read Bugfixes changes"]');
    await page.wait("document.querySelector('.pn-bugfixes').open");
    await page.text(".pn-bugfixes", "Fixed a bug");
    assert.equal(await page.evaluate("document.querySelectorAll('.pn-bugfix-list').length"), 1);
    assert.equal(await page.evaluate("document.querySelectorAll('.pn-bugfix-list > li').length"), 2);
    assert.equal(await page.evaluate("document.querySelectorAll('.pn-bugfixes .pn-entry').length"), 0, "bugfixes are compact rows, not cards");
    assert.equal(await page.evaluate("document.querySelectorAll('.pn-bugfix-wrench').length"), 1);
    await page.wait("(() => { const icon = document.querySelector('.pn-bugfix-list .champion-icon'); return icon?.complete && icon.naturalWidth > 0; })()", "named bugfix uses a champion icon");
    await page.wait("document.querySelector('.pn-checkpoint[aria-current=location]').getAttribute('aria-controls') === 'pn-bugfixes'");
    await page.wait("(() => { const main = document.querySelector('.dashboard-main'); return Math.abs(main.scrollTop + main.clientHeight - main.scrollHeight) < 2; })()", "the final checkpoint jump clamps to the bottom");
    await page.cdp.send("Emulation.setDeviceMetricsOverride", { width: 1280, height: 2000, deviceScaleFactor: 1, mobile: false });
    await page.settle();
    await page.click('.pn-checkpoint[aria-label="Read Azir changes"]');
    // The overview sits above the cards, so the first card's jump may stop short of zero; the rail still points at it.
    await page.wait("document.querySelector('.pn-checkpoint[aria-current=location]').getAttribute('aria-controls') === 'pn-champions-0'");
    await page.evaluate("document.querySelector('.dashboard-main').scrollTo({ top: 0, behavior: 'instant' })");
    await page.wait("document.querySelector('.dashboard-main').scrollTop === 0 && document.querySelector('.pn-checkpoint[aria-current=location]').getAttribute('aria-controls') === 'pn-champions-0'");
    await page.wait("(() => { const marker = document.querySelector('.pn-rail-marker').getBoundingClientRect(); const first = document.querySelector('.pn-checkpoint').getBoundingClientRect(); return Math.abs(marker.top + marker.height / 2 - first.top - first.height / 2) < 1; })()", "the marker is pinned to the first checkpoint at scroll zero");
    await page.evaluate("document.querySelector('.dashboard-main').scrollTop = document.querySelector('.dashboard-main').scrollHeight");
    await page.wait("document.querySelector('.pn-checkpoint[aria-current=location]').getAttribute('aria-controls') === 'pn-bugfixes'", "the bottom selects the final checkpoint even in a tall viewport");
    await page.cdp.send("Emulation.setDeviceMetricsOverride", { width: 1280, height: 820, deviceScaleFactor: 1, mobile: false });
    await page.settle();
    await page.evaluate("document.querySelector('.pn-bugfixes').open = false; document.querySelector('.dashboard-main').scrollTop = 0");
    await page.cdp.send("Emulation.setEmulatedMedia", { features: [] });
  });
  await check("Patch notes current and previous", async () => {
    assert.equal(await page.evaluate("document.querySelectorAll('.pn-patch-switch button').length"), 2);
    await page.text('.pn-patch-switch [aria-pressed="true"]', "26.21");
    await page.click('.pn-patch-switch [aria-label="Patch 26.20, Previous"]');
    await page.text(".pn-patch-line h2", "Patch 26.20");
    await page.text('.pn-patch-switch [aria-pressed="true"]', "26.20");
    await page.text(".pn-patch-line time", "6 October 2026");
    await page.text('.pn-entry[aria-label="Caitlyn"] .pn-tag', "Buff");
    await page.text('.pn-entry[aria-label="Caitlyn"] h4', "Q - Piltover Peacemaker");
    assert.equal(await page.evaluate("document.querySelector('.pn-entry[aria-label=\"Lee Sin\"] .pn-after .is-buff').textContent"), "10");
    assert.equal(await page.evaluate("document.querySelectorAll('.pn-checkpoint').length"), 5);
    assert.equal(await page.evaluate("document.querySelector('.dashboard-main').scrollTop"), 0, "switching starts at the new patch header");
    await screenshot("patch-notes-previous");
    await page.click('.pn-checkpoint[aria-label="Read Caitlyn changes"]');
    await page.wait("document.querySelector('.dashboard-main').scrollTop > 0");
    await page.click('.pn-refresh');
    await page.settle();
    await page.text(".pn-patch-line h2", "Patch 26.20");
    await page.click('.pn-patch-switch [aria-label="Patch 26.21, Current"]');
    await page.text(".pn-patch-line h2", "Patch 26.21");
    assert.equal(await page.evaluate("document.querySelectorAll('.pn-checkpoint').length"), 8);
    await page.click('.pn-patch-switch [aria-label="Patch 26.20, Previous"]');
    await page.text(".pn-patch-line h2", "Patch 26.20");
    await nav("Dashboard", ".match-row");
    await nav("Patch notes", ".pn-entry");
    await page.text(".pn-patch-line h2", "Patch 26.21");
    await page.text('.pn-patch-switch [aria-pressed="true"]', "26.21");
  });
  await check("Active sidebar entries scroll every page back to the top", async () => {
    for (const [label, selector] of [["Patch notes", ".pn-entry"], ["Dashboard", ".match-row"], ["Tierlist", ".tier-table"], ["Matchups", ".fa-matchup-empty"], ["Draft", ".fa-draft-empty"], ["Accounts", ".fa-account-card"], ["Settings", ".fa-settings-content"]]) {
      await nav(label, selector);
      const reduce = label !== "Patch notes";
      await page.cdp.send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: reduce ? "reduce" : "no-preference" }] });
      // Ensure even short empty pages have a real scroll range for this shared behavior.
      await page.evaluate(`(() => {
        const main = document.querySelector('.dashboard-main') ?? document.querySelector('.full-app-embed .main-panel');
        const spacer = document.createElement('div'); spacer.id = 'sidebar-scroll-probe'; spacer.style.minHeight = '1000px'; spacer.style.flexShrink = '0'; main.append(spacer);
        const scroll = main.scrollTo; main.scrollTo = function(options) { window.__sidebarScroll = options; scroll.call(this, options); };
        main.scrollTop = 200; window.__sidebarScroll = null;
      })()`);
      assert.equal(await page.evaluate("(document.querySelector('.dashboard-main') ?? document.querySelector('.full-app-embed .main-panel')).scrollTop > 0"), true);
      await page.click('nav[aria-label="Swapper pages"] button', label);
      await page.wait("(document.querySelector('.dashboard-main') ?? document.querySelector('.full-app-embed .main-panel')).scrollTop === 0");
      assert.deepEqual(await page.evaluate("window.__sidebarScroll"), { top: 0, behavior: reduce ? "instant" : "smooth" });
      assert.equal(await page.evaluate("Boolean(document.getElementById('sidebar-scroll-probe'))"), true, "same-page clicks preserve page content");
      await page.evaluate("document.getElementById('sidebar-scroll-probe').remove(); delete (document.querySelector('.dashboard-main') ?? document.querySelector('.full-app-embed .main-panel')).scrollTo");
    }
    await page.cdp.send("Emulation.setEmulatedMedia", { features: [] });
    await navigate(fullApp, ".nav-group");
  });
  await check("Patch notes ability fallbacks", async () => {
    await navigate(`${fullApp}?patch-abilities-offline`, ".nav-group");
    await nav("Patch notes", ".pn-entry");
    await page.text('.pn-entry[aria-label="Lee Sin"] h4', "E - Tempest");
    assert.deepEqual(await page.evaluate("Array.from(document.querySelectorAll('.pn-entry[aria-label=\"Caitlyn\"] h4')).map(h => h.textContent)"), ["Passive", "Q"]);
    assert.equal(await page.evaluate("document.querySelector('.pn-ability-icon') === null"), true, "unavailable metadata leaves readable source names or bare keys");
    await navigate(fullApp, ".nav-group");
  });
  await check("Sidebar Draft", () => nav("Draft", ".fa-draft-empty"));
  await check("Sidebar Accounts", async () => {
    await nav("Accounts", ".fa-account-card");
    await page.text('.fa-account-card.is-active .fa-restart', 'Restart client');
  });
  await check("Sidebar Settings", () => nav("Settings", ".fa-settings-content"));
  // Deceive only applies after a client restart; the backend plan decides (mocked via ?restart=).
  await check("Deceive toggle restarts the client", async () => {
    await page.click('#deceive-switch');
    await page.text('.switch-overlay h2', 'Restarting client');
    await page.wait("!document.querySelector('.switch-overlay')");
    for (const [plan, text, screenshotName] of [["ask", "Restart the client now", "deceive-confirm"], ["defer", "applies after the game", "deceive-deferred"]]) {
      await navigate(`${fullApp}?restart=${plan}`, ".nav-group");
      await nav("Settings", ".fa-settings-content");
      await page.click('#deceive-switch');
      await page.text('.deceive-notice', text);
      assert.equal(await page.evaluate("document.querySelector('.switch-overlay') === null"), true);
      await screenshot(screenshotName);
      await screenshot(screenshotName, 1440, 900);
    }
    await navigate(fullApp, ".nav-group");
  });
  await check("Sidebar Dashboard", () => nav("Dashboard", ".match-row"));
  // Search owns these scenarios; all identities are invented preview fixtures.
  async function key(key, modifiers = 0) {
    const code = { Enter: 13, Escape: 27, ArrowDown: 40, ArrowUp: 38, ArrowLeft: 37, ArrowRight: 39, Tab: 9 }[key] ?? 0;
    await page.cdp.send('Input.dispatchKeyEvent', { type: 'keyDown', key, modifiers, windowsVirtualKeyCode: code });
    await page.cdp.send('Input.dispatchKeyEvent', { type: 'keyUp', key, modifiers, windowsVirtualKeyCode: code });
  }
  const searchInput = '.title-location input';
  await check('Location search opens in place by pill click, Esc and blur leave search', async () => {
    const locationBox = () => page.evaluate("JSON.stringify(document.querySelector('.title-location').getBoundingClientRect().toJSON())");
    const before = await locationBox();
    // The current segment and empty pill area belong to the Search button.
    const label = await page.evaluate("(() => { const r = document.querySelector('.location-trail [aria-current=\"page\"]').getBoundingClientRect(); return { x: r.x + r.width / 2, y: r.y + r.height / 2 }; })()");
    await cdp.send('Input.dispatchMouseEvent', { type: 'mousePressed', ...label, button: 'left', clickCount: 1 });
    await cdp.send('Input.dispatchMouseEvent', { type: 'mouseReleased', ...label, button: 'left', clickCount: 1 });
    await page.visible('input[role="combobox"]:focus');
    assert.equal(await locationBox(), before);
    assert.equal(await page.evaluate("document.querySelector('.title-location input').getAttribute('aria-controls')"), 'palette-results');
    const anchor = await page.evaluate("({input: document.querySelector('.title-location').getBoundingClientRect().bottom, results: document.querySelector('.palette').getBoundingClientRect().top})");
    assert.ok(anchor.results > anchor.input && anchor.results - anchor.input < 12);
    await key('Escape');
    await page.visible('.location-search-button:focus');
    await page.text('.location-trail [aria-current="page"]', 'Dashboard');
    await key('k', 2);
    await page.visible(searchInput);
    await page.evaluate("document.querySelector('nav[aria-label=\"Swapper pages\"] button').focus()");
    await page.wait("!document.querySelector('.palette')", 'blur closes search');
    assert.equal(await page.evaluate("document.activeElement.closest('nav').getAttribute('aria-label')"), 'Swapper pages');
    await page.click('.location-search-button');
    await page.click('nav[aria-label="Swapper pages"] button', 'Dashboard');
    await page.wait("!document.querySelector('.palette')", 'outside click closes search');
  });
  await check('Palette icons, groups, focus and empty results', async () => {
    await page.evaluate("document.querySelector('.location-search-button').focus()");
    await key('k', 2);
    await page.visible('.palette');
    await page.text('.palette h3', 'Pages');
    assert.equal(await page.evaluate("getComputedStyle(document.querySelector('.title-location input')).outlineStyle"), 'none');
    assert.ok(await page.evaluate("document.querySelector('.palette-nav-icon svg') !== null"));
    await page.type(searchInput, 'nothing-will-match');
    await page.text('.palette-empty', 'No results found');
    assert.equal(await page.evaluate("document.querySelector('.title-location input').getAttribute('aria-activedescendant')"), null);
    await key('ArrowDown');
    await key('Escape');
    await page.visible('.location-search-button:focus');
  });
  await check('Palette champion selection and local recents', async () => {
    await page.click('.location-search-button');
    await page.type(searchInput, 'Ahri');
    await page.visible('.palette-option .champion-icon');
    await key('Enter');
    await page.text('.champion-hero h2', 'Ahri');
    await page.text('.location-trail [aria-current="page"]', 'Ahri');
    await page.click('.location-search-button');
    await page.text('.palette h3', 'Recent');
    await page.click('.palette-hint button', 'Clear recent');
    assert.equal(await page.evaluate("document.querySelector('[role=group][aria-label=Recent]')"), null);
    await key('Escape');
  });
  await check('Partial names appear while typing without Enter', async () => {
    await navigate(fullApp + '?search-slow', '.match-row');
    await page.click('.location-search-button');
    await page.type(searchInput, 'pLaYeR 1');
    await page.text('.palette-player-copy', 'Player 1#DEMO');
    assert.equal(await page.evaluate("document.querySelector('.player-page')"), null);
    await page.text('.palette-hint', 'Searching players');
    assert.equal(await page.evaluate('window.liveChecks.length'), 0);
    await screenshot('search-typing-results');
    await key('Escape');
  });
  await check('Online tagless suggestions and stale responses', async () => {
    await navigate(fullApp + '?search-reorder', '.match-row');
    await page.click('.location-search-button');
    await page.type(searchInput, 'exam');
    await page.wait("window.searchCalls.some(c => c.online && c.query === 'exam')", 'online debounce');
    await page.evaluate("document.querySelector('.title-location input').select()");
    await page.cdp.send('Input.insertText', { text: 'Player 2' });
    await page.text('.palette-player-copy', 'Player 2#DEMO');
    await page.wait("window.searchCalls.some(c => c.online && c.query === 'Player 2')", 'new query requested');
    await page.wait("!document.querySelector('.palette-hint').textContent.includes('Searching')", 'new query settled');
    await page.wait("window.searchCalls.some(c => c.query === 'exam' && c.online && c.done)", 'stale response finished');
    assert.equal(await page.evaluate("document.querySelector('.palette-player-copy').textContent.includes('Example')"), false);
    await page.evaluate("document.querySelector('.title-location input').select()");
    await page.cdp.send('Input.insertText', { text: 'Example' });
    await page.text('.palette-player-copy', 'Example#TAG');
    assert.equal(await page.evaluate('window.liveChecks.length'), 0);
    await key('Enter');
    await page.text('.player-identity h2', 'Example#TAG');
  });
  await check('Profile LIVE badge opens a read-only ten-player view', async () => {
    await navigate(fullApp + '?profile-live', '.match-row');
    await page.click('.location-search-button');
    await page.type(searchInput, 'Player 1');
    await page.text('.palette-player-copy', 'Player 1#DEMO');
    await page.visible('.palette-option .nav-badge.live');
    await key('Enter');
    await page.visible('.player-live-badge');
    await screenshot('profile-live-badge');
    await page.click('.player-live-badge');
    await page.visible('.player-live-game .lg-row');
    assert.equal(await page.evaluate("document.querySelectorAll('.player-live-game .lg-row').length"), 10);
    assert.equal(await page.evaluate("document.querySelectorAll('.player-live-game .lg-live').length"), 0);
    await page.click('.player-live-game .lg-row');
    await page.visible('.lg-profile');
    assert.equal(await page.evaluate("document.querySelector('.lg-profile .cd-sheet')"), null);
    await page.click('.lg-close');
    await screenshot('profile-live-players', 1440, 1400);
    await page.click('.player-live-game .player-back');
    await page.visible('.player-identity');
    await navigate(fullApp, '.match-row');
  });
  await check('Live roster failures keep the profile usable', async () => {
    await navigate(fullApp + '?profile-live&profile-live-fail', '.match-row');
    await page.click('.location-search-button');
    await page.type(searchInput, 'Player 1');
    await page.text('.palette-player-copy', 'Player 1#DEMO');
    await key('Enter');
    await page.click('.player-live-badge');
    await page.text('.player-live-game', 'Live players unavailable');
    await page.click('.player-live-game .player-back');
    await page.visible('.player-identity');
    await navigate(fullApp, '.match-row');
  });
  await check('Palette player profile, match details and back', async () => {
    await nav('Dashboard', '.match-row');
    await page.click('.location-search-button');
    await page.type(searchInput, 'Player 1#DEMO');
    await page.visible('.palette-option .search-profile-icon');
    await page.text('.palette-player-copy', 'Played with you');
    await key('Enter');
    await page.text('.player-identity h2', 'Player 1#DEMO');
    await page.text('.location-trail button', 'Player');
    await page.text('.location-trail [aria-current="page"]', 'Player 1#DEMO');
    assert.equal(await page.evaluate("document.querySelector('.palette')"), null);
    await page.visible('.player-games .match-row');
    // ---- profile2: shared roles, honest rank and cached teammate portraits ----
    await page.text('.player-ranks', 'Solo/Duo');
    assert.equal(await page.evaluate("document.querySelector('.player-ranks').textContent.includes('Flex')"), false);
    // ---- profile round9: aggregates, progressive history and responsive layout ----
    assert.equal(await page.evaluate("document.querySelectorAll('.player-metric').length"), 5);
    assert.equal(await page.evaluate("document.querySelectorAll('.player-champions li').length"), 3);
    await page.text('.player-champions', '12 loaded games');
    await page.text('.player-together', '12 games');
    assert.equal(await page.evaluate("document.querySelectorAll('.player-roles .role-strip > div').length"), 5);
    assert.equal(await page.evaluate("document.querySelector('.player-roles [data-tooltip=\"Jungle: 7W 5L\"] b').textContent"), '58%');
    assert.equal(await page.evaluate("document.querySelectorAll('.player-roles .is-empty').length"), 4);
    assert.equal(await page.evaluate("document.querySelectorAll('.player-together img.search-profile-icon').length"), 4);
    assert.equal(await page.evaluate("document.querySelectorAll('.player-champion-art').length"), 3);
    assert.equal(await page.evaluate("document.querySelectorAll('.player-games .match-row').length"), 10);
    for (const [width, height] of [[1920, 1080], [1440, 900], [1280, 820]]) {
      await page.cdp.send('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 1, mobile: false });
      await page.settle();
      assert.deepEqual(await page.evaluate(`(${horizontalOverflow.toString()})()`), []);
      assert.equal(await page.evaluate("document.querySelector('.player-layout').getBoundingClientRect().width <= document.querySelector('.dashboard-main').clientWidth"), true);
      const columns = await page.evaluate("getComputedStyle(document.querySelector('.player-layout')).gridTemplateColumns.split(' ').length");
      assert.equal(columns, width >= 1440 ? 2 : 1);
    }
    await page.click('.player-more');
    assert.equal(await page.evaluate("document.querySelectorAll('.player-games .match-row').length"), 12);
    assert.equal(await page.evaluate("document.querySelector('.player-more')"), null);
    await page.click('.player-together li:first-child button');
    await page.text('.player-identity h2', 'Player 2#DEMO');
    await page.click('.player-back');
    await page.text('.player-identity h2', 'Player 1#DEMO');
    // ---- end profile2 checks ----
    await page.click('.player-games .match-champ');
    // ---- matchdetail: player-perspective match also opens Overview ----
    await page.visible('.md-comparison');
    await page.text('.location-trail button', 'Player 1#DEMO');
    await page.text('.location-trail [aria-current="page"]', 'Lee Sin · Victory');
    // ---- end matchdetail player perspective ----
    await page.click('.location-trail button', 'Player 1#DEMO');
    await page.visible('.player-identity');
    await nav('Dashboard', '.match-row');
  });
  await check('Location history supports compact buttons and Alt left/right', async () => {
    await nav('Accounts', '.fa-account-card');
    await key('ArrowLeft', 1);
    await page.text('.location-trail [aria-current="page"]', 'Dashboard');
    await key('ArrowRight', 1);
    await page.text('.location-trail [aria-current="page"]', 'Accounts');
    await page.click('button[aria-label="Back"]');
    await page.text('.location-trail [aria-current="page"]', 'Dashboard');
    await page.click('button[aria-label="Forward"]');
    await page.text('.location-trail [aria-current="page"]', 'Accounts');
    await nav('Dashboard', '.match-row');
  });
  await check('Long location trails truncate the middle, keep their tooltip and respect reduced motion', async () => {
    await navigate(`${fullApp}?locbar-long`, '.match-row');
    await page.click('.location-search-button');
    await page.type(searchInput, 'A very long');
    await page.visible('.palette-player-copy');
    await key('Enter');
    await page.visible('.player-games .match-row');
    await page.click('.player-games .match-champ');
    await page.visible('.md-comparison');
    await page.text('.location-trail [aria-current="page"]', 'Lee Sin · Victory');
    for (const [width, height] of [[1920, 1080], [1280, 820]]) {
      await cdp.send('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 1, mobile: false });
      const trail = await page.evaluate(`(() => {
        const view = document.querySelector('.location-view'), middle = view.querySelector('li:nth-child(2) button');
        return { tooltip: view.dataset.tooltip, label: middle.textContent, clipped: middle.scrollWidth > middle.clientWidth,
          ellipsis: getComputedStyle(middle).textOverflow,
          fits: view.querySelector('ol').getBoundingClientRect().right < view.querySelector('kbd').getBoundingClientRect().left };
      })()`);
      assert.equal(trail.tooltip, `Player › ${trail.label} › Lee Sin · Victory`);
      assert.equal(trail.clipped, true);
      assert.equal(trail.ellipsis, 'ellipsis');
      assert.equal(trail.fits, true);
    }
    await cdp.send('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-reduced-motion', value: 'reduce' }] });
    assert.equal(await page.evaluate("getComputedStyle(document.querySelector('.location-trail')).animationName"), 'none');
    await key('k', 2);
    await page.visible(searchInput);
    assert.equal(await page.evaluate("getComputedStyle(document.querySelector('.location-search')).animationName"), 'none');
    await key('Escape');
    await cdp.send('Emulation.setEmulatedMedia', { features: [] });
    await navigate(fullApp, '.match-row');
  });
  await check('Palette no-client saved players and keyboard across groups', async () => {
    await navigate(`${fullApp}?stale`, '.match-row');
    await page.click('.location-search-button');
    await page.type(searchInput, 'Player 5');
    await page.text('.palette-player-copy', 'Played against you');
    await page.text('.palette-player-status', 'Open League Client');
    await key('Enter');
    await page.text('.player-page .ui-notice', 'Open League Client');
    await page.visible('.player-games .match-row');
    await page.click('.location-search-button');
    const recentCount = await page.evaluate("document.querySelectorAll('.palette [role=group][aria-label=Recent] [role=option]').length");
    for (let i = 0; i < recentCount; i++) await key('ArrowDown');
    await key('Enter');
    await page.text('.ui-page-header h1', 'Dashboard');
    await page.click('.location-search-button');
    await page.click('.palette-hint button', 'Clear recent');
    await key('Escape');
    await navigate(fullApp, '.match-row');
  });
  // ---- profile2: shared role tooltips/empty states and missing season record ----
  await check('Player missing record keeps tier and LP with five empty roles', async () => {
    await navigate(fullApp + '?profile-no-record&profile-no-roles', '.match-row');
    await page.click('.location-search-button');
    await page.type(searchInput, 'Player 1#DEMO');
    await key('Enter');
    await page.visible('.player-games .match-row');
    await page.text('.player-ranks', 'Emerald II');
    await page.text('.player-ranks', '40');
    // The MMR estimate follows the profile and lands on its rank card.
    await page.text('.player-ranks .mmr-line', 'Emerald I');
    await page.text('.player-ranks', 'Season record unavailable');
    assert.equal(await page.evaluate("document.querySelectorAll('.player-rank-record').length"), 0);
    assert.equal(await page.evaluate("document.querySelector('.player-ranks').textContent.includes('win rate')"), false);
    assert.equal(await page.evaluate("document.querySelectorAll('.player-roles .is-empty').length"), 5);
    assert.equal(await page.evaluate("document.querySelector('.player-roles [data-tooltip=\"Jungle: 0W 0L\"] b').textContent"), '–');
    assert.equal(await page.evaluate("document.querySelectorAll('.player-roles meter').length"), 0);
    await page.click('.player-back');
    assert.equal(await page.evaluate("document.querySelectorAll('.role-strip > div').length"), 5);
    assert.equal(await page.evaluate("document.querySelector('.role-strip [data-tooltip=\"Top: 7W 3L\"] b').textContent"), '70%');
  });
  // ---- end profile2 missing data checks ----
  // ---- record: u.gg season-record fallback (mock resolves it) ----
  await check('Player record from the u.gg fallback shows a source label', async () => {
    await navigate(fullApp + '?profile-ugg-record', '.match-row');
    await page.click('.location-search-button');
    await page.type(searchInput, 'Player 1#DEMO');
    await key('Enter');
    await page.visible('.player-games .match-row');
    await page.text('.player-ranks', 'win rate');
    await page.text('.player-record-source', 'via u.gg');
    assert.equal(await page.evaluate("document.querySelector('.player-record-source').dataset.tooltip"), "Your League Client didn't return this player's season record.");
  });
  // ---- end record checks ----

  await check('Player partial data keeps games and offers retry', async () => {
    await navigate(`${fullApp}?search-partial`, '.match-row');
    await page.click('.location-search-button');
    await page.type(searchInput, 'Player 1#DEMO');
    await page.visible('.palette-player-copy');
    await key('Enter');
    await page.text('.player-page .ui-notice', 'Some profile data could not be loaded');
    await page.visible('.player-games .match-row');
    await page.visible('.player-page .ui-notice button:not(:disabled)');
  });
  await check('Player lookup failure keeps saved rows and offers retry', async () => {
    await navigate(`${fullApp}?search-error`, '.match-row');
    await page.click('.location-search-button');
    await page.type(searchInput, 'Player 1#DEMO');
    await page.visible('.palette-player-copy');
    await key('Enter');
    await page.text('.player-page .ui-notice', 'This profile is unavailable');
    await page.visible('.player-games .match-row');
    await page.click('.player-page .ui-notice button', 'Try again');
    await page.visible('.player-page .ui-notice button:not(:disabled)');
    await navigate(fullApp, '.match-row');
  });
  await check("Sidebar selector opens flat account picker", async () => {
    await page.click(".podium-switch");
    await page.visible('[role="dialog"][aria-label="Switch account"]');
    assert.equal(await page.evaluate("document.querySelectorAll('.picker-row').length"), 3);
    await page.visible('.podium-switch[aria-expanded="true"]');
    await page.text('.picker-row.is-current .picker-tier', 'Emerald II');
    await page.text('.picker-copy', 'Alt Account');
    assert.equal(await page.evaluate("document.querySelectorAll('.account-picker h2, .picker-group').length"), 0);
    await page.cdp.send('Input.dispatchKeyEvent', { type: 'keyDown', key: 'Escape', code: 'Escape', windowsVirtualKeyCode: 27 });
    await page.wait("!document.querySelector('.account-picker')");
    assert.equal(await page.evaluate("document.activeElement.classList.contains('podium-switch')"), true);
    await page.cdp.send('Input.dispatchKeyEvent', { type: 'keyUp', key: 'Escape', code: 'Escape', windowsVirtualKeyCode: 27 });
    await page.click('.podium-switch');
    await page.visible('.account-picker');
    await audit();
    await screenshot("account-picker");
    await screenshot("account-picker", 1440, 900);
    // Switching onto the signed-in account restarts the client (same backend path as the tray flyout).
    await page.text('.picker-restart', 'Restart client');
    assert.equal(await page.evaluate("document.querySelectorAll('.picker-row.is-current + .picker-restart').length"), 1);
    await page.click('.picker-restart');
    await page.text('.switch-overlay h2', 'Restarting client');
    await screenshot("restart-overlay");
    await page.wait("!document.querySelector('.switch-overlay')");
    await page.click('.podium-switch');
    await page.visible('.account-picker');
    await page.click(".picker-foot button", "Manage accounts");
    await page.visible(".fa-account-card");
  });
  // Every restart entry point asks the backend plan first (mocked via ?restart=); the status offer is mocked via ?client=.
  await check("Picker restart action explains itself and is guarded", async () => {
    await navigate(`${fullApp}?restart=ask`, ".nav-group");
    await page.click(".podium-switch");
    await page.evaluate("document.querySelector('.picker-restart').focus()");
    await screenshot("restart-picker-action", 1920, 1080);
    await page.click('.picker-restart');
    await page.text('.restart-dialog h2', 'Restart during champion select?');
    assert.equal(await page.evaluate("document.querySelector('.switch-overlay') === null"), true);
    await screenshot("restart-confirm", 1920, 1080);
    await page.click('.restart-dialog .fa-primary', 'Restart anyway');
    await page.text('.switch-overlay h2', 'Restarting client');
    await page.wait("!document.querySelector('.switch-overlay')");
    await navigate(`${fullApp}?restart=defer`, ".nav-group");
    await page.click(".podium-switch");
    await page.click('.picker-restart');
    await page.text('.restart-dialog h2', "Can't restart during a game");
    assert.equal(await page.evaluate("document.querySelector('.restart-dialog .fa-primary') === null"), true);
    await page.click('.restart-dialog button', 'OK');
    await page.wait("!document.querySelector('.restart-dialog')");
    assert.equal(await page.evaluate("document.querySelector('.switch-overlay') === null"), true);
    await navigate(`${fullApp}?restart=ask`, ".nav-group");
    await nav("Accounts", ".fa-account-card");
    await page.click('.fa-account-card.is-active .fa-restart');
    await page.text('.restart-dialog h2', 'Restart during champion select?');
    await page.click('.restart-dialog button', 'Cancel');
    await navigate(fullApp, ".nav-group");
  });
  await check("Ctrl+K palette restarts the client", async () => {
    await page.evaluate("document.querySelector('.location-search-button').focus()");
    await key('k', 2);
    await page.visible('.palette');
    await page.type(searchInput, 'restart');
    await page.text('.palette-option', 'Restart client');
    await screenshot("restart-palette", 1920, 1080);
    await key('Enter');
    await page.text('.switch-overlay h2', 'Restarting client');
    await page.wait("!document.querySelector('.switch-overlay')");
  });
  await check("Closed client offers Open League across account actions", async () => {
    await navigate(`${fullApp}?client=closed`, ".sidebar-play");
    await nav("Accounts", ".fa-account-card");
    await page.text('.fa-account-card.is-active .fa-restart', 'Open League');
    await page.click('.podium-switch');
    await page.text('.picker-restart', 'Open League');
    await key('Escape');
    await page.wait("!document.querySelector('.account-picker')");
    await page.evaluate("document.querySelector('.location-search-button').focus()");
    await key('k', 2);
    await page.visible('.palette');
    await page.type(searchInput, 'open league');
    await page.text('.palette-option', 'Open League');
    for (const query of ['restart', 'restart client', 'restart riot client']) {
      await page.evaluate("document.querySelector('.title-location input').select()");
      await page.cdp.send('Input.insertText', { text: query });
      await page.text('.palette-option', 'Open League');
    }
    await page.evaluate("window.__TAURI_INTERNALS__.invoke('plugin:event|emit', { event: 'client_health', payload: 'ok' })");
    await page.text('.palette-option', 'Restart client');
    await page.text('.fa-account-card.is-active .fa-restart', 'Restart client');
    await page.evaluate("window.__TAURI_INTERNALS__.invoke('plugin:event|emit', { event: 'client_health', payload: 'closed' })");
    await page.text('.palette-option', 'Open League');
    await page.text('.fa-account-card.is-active .fa-restart', 'Open League');
    await key('Enter');
    await page.text('.switch-overlay p', 'Signing in through Riot Client.');
    await page.wait("!document.querySelector('.switch-overlay')");
    await navigate(fullApp, ".nav-group");
  });
  await check("Not-responding status offers a guarded restart", async () => {
    await navigate(fullApp, ".nav-group");
    assert.equal(await page.evaluate("document.querySelector('.podium-status') === null"), true);
    for (const state of ["hung", "dead"]) {
      await navigate(`${fullApp}?client=${state}&restart=ask`, ".podium-status");
      await page.text('.podium-status', "League Client isn't responding");
      if (state === "hung") await screenshot("restart-not-responding", 1920, 1080);
      await page.click('.podium-status button', 'Restart client');
      await page.text('.restart-dialog h2', 'Restart during champion select?');
      await page.click('.restart-dialog button', 'Cancel');
    }
    await navigate(`${fullApp}?client=closed`, ".sidebar-play");
    await page.text('.sidebar-play', 'Play');
    assert.equal(await page.evaluate("document.querySelector('.podium-status') === null"), true);
    // Hidden while Swapper is switching.
    await navigate(`${fullApp}?client=hung&switching`, ".switch-overlay");
    assert.equal(await page.evaluate("document.querySelector('.podium-status') === null"), true);
    await navigate(fullApp, ".nav-group");
  });

  for (const [query, selector, expected] of [
    ["stale", ".match-row", null],
    ["loading", '[aria-label="Loading today"]', null],
    ["disconnected", ".ui-empty", "Sign in to League Client"],
    ["empty", ".ui-empty", "No games yet"],
    ["live=1", ".lg-team", "Blue team"],
  ]) {
    await check(`Full app ?${query}`, async () => {
      await navigate(`${fullApp}?${query}`, selector);
      await shell();
      if (expected) await page.text(selector, expected);
      if (query === "stale") {
        await page.visible(".podium-dot.is-off");
        await page.text(".ui-page-actions", "Updated 2h ago");
      }
      if (query === "live=1") {
        await nav("Draft", ".lg-team");
        assert.equal(await page.evaluate("document.querySelectorAll('.lg-row').length"), 10);
      }
    });
  }

  await check("Live game columns align with long names, badges and hidden players", async () => {
    for (const pregame of [false, true]) {
      await navigate(`${fullApp}?live=1&live-stress${pregame ? '&live-pregame' : ''}`, '.lg-row');
      assert.equal(await page.evaluate("document.querySelectorAll('.lg-team.is-red .lg-row')[2].querySelectorAll('.lg-badge').length"), 3);
      await page.text('.lg-team.is-blue .lg-dmg', 'AD 75%');
      for (const width of [1624, 1280, 1100, 900, 800]) {
        await page.cdp.send('Emulation.setDeviceMetricsOverride', { width, height: 1100, deviceScaleFactor: 1, mobile: false });
        await page.settle();
        const errors = await page.evaluate(`(() => {
          const errors = [];
          for (const team of document.querySelectorAll('.lg-team')) {
            const header = team.querySelector('.lg-cols');
            const columns = getComputedStyle(header).gridTemplateColumns;
            const wide = getComputedStyle(header).display !== 'none';
            for (const row of team.querySelectorAll('.lg-row')) {
              if (Math.abs(row.getBoundingClientRect().width - team.querySelector('ol').getBoundingClientRect().width) > 1) errors.push('row does not fill the team');
              if (wide && getComputedStyle(row).gridTemplateColumns !== columns) errors.push('row columns differ from header');
              for (const cell of row.children) {
                const bounds = cell.getBoundingClientRect();
                if (bounds.right > row.getBoundingClientRect().right + 1) errors.push('cell overflows row');
                if (wide && !cell.classList.contains('lg-streamer')) {
                  const index = [...row.children].indexOf(cell);
                  const column = header.children[row.classList.contains('is-hidden') && index === 2 ? 3 : index];
                  const expected = column.getBoundingClientRect();
                  if (bounds.left < expected.left - 1 || bounds.right > expected.right + 1) errors.push('cell outside its header column');
                }
              }
            }
          }
          return errors;
        })()`);
        assert.deepEqual(errors, [], `${pregame ? 'pregame' : 'live'} at ${width}px`);
        assert.deepEqual(await page.evaluate(`(${horizontalOverflow.toString()})()`), []);
        if ([1624, 1280, 900].includes(width)) await screenshot(`live-columns-${pregame ? 'pregame' : 'live'}`, width, 1100);
        if (capture && width === 900) {
          await page.evaluate("document.querySelector('.lg-team.is-red').scrollIntoView({ block: 'end' })");
          await screenshot(`live-columns-${pregame ? 'pregame' : 'live'}-red`, width, 1100);
          await page.evaluate("document.querySelector('.lg-team.is-blue').scrollIntoView({ block: 'start' })");
        }
      }
    }
    await page.cdp.send('Emulation.setDeviceMetricsOverride', { width: 1280, height: 820, deviceScaleFactor: 1, mobile: false });
  });

  await check("Draft follows champ select, loading, live, reconnect and game end", async () => {
    await navigate(fullApp, ".nav-group");
    await nav("Draft", ".fa-draft-empty");
    await page.evaluate("window.previewGamePhase('ChampSelect')");
    await page.visible(".cs-band");
    for (const phase of ["GameStart", "InProgress", "Reconnect"]) {
      await page.evaluate(`window.previewGamePhase('${phase}')`);
      await page.visible(".lg-team");
      await page.text('.ui-page-header h1', 'Live game');
      await page.text('nav[aria-label="Swapper pages"] [aria-current="page"]', "Draft");
      assert.equal(await page.evaluate("document.querySelectorAll('.lg-row').length"), 10);
    }
    await nav("Dashboard", ".match-row");
    await page.evaluate("window.previewNavigate('live')");
    await page.visible(".lg-team");
    await page.text('nav[aria-label="Swapper pages"] [aria-current="page"]', "Draft");
    await page.evaluate("window.previewGamePhase('EndOfGame')");
    await page.text('nav[aria-label="Swapper pages"] [aria-current="page"]', "Dashboard");
    await nav("Draft", ".fa-draft-empty");
    await shell();
  });

  await check("Unsupported draft and live games show the SR note without players", async () => {
    await navigate(fullApp, ".nav-group");
    await page.evaluate("window.previewGamePhase('ChampSelect', false)");
    await page.text(".fa-draft-page", "Swapper's draft and live game work for Summoner's Rift games.");
    assert.equal(await page.evaluate("document.querySelectorAll('.cs-slot').length"), 0);
    await page.evaluate("window.previewGamePhase('GameStart', false)");
    await page.text(".lg-empty", "Swapper's draft and live game work for Summoner's Rift games.");
    assert.equal(await page.evaluate("document.querySelectorAll('.lg-row').length"), 0);
  });

  const draftGeometry = () => page.evaluate(`(() => {
    const selectors = ['.cs-band', '.cs-band-head', '.cs-band-phase', '.cs-band-side', '.cs-bans', '.cs-bans > span', '.cs-teams', '.cs-team', '.cs-slot', '.cs-slot-role', '.cs-draft-clock', '.cs-draft-clock > *', '.cs-workspace'];
    // Page coordinates, so a sub-pixel scroll from focusing a button is not mistaken for a layout change.
    const scroll = document.querySelector('.dashboard-main')?.scrollTop ?? 0;
    return Object.fromEntries(selectors.map(selector => [selector, Array.from(document.querySelectorAll(selector), el => {
      const r = el.getBoundingClientRect();
      return [r.x, r.y + scroll, r.width, r.height].map(value => Math.round(value));
    })]));
  })()`);
  let draftLayout;
  await check("Roleless draft keeps its main lane and loads other main runes", async () => {
    await navigate(`${fullApp}?draft-guide`, '.match-row');
    await page.evaluate(`(async () => {
      localStorage.setItem('cs-role', 'mid');
      const invoke = window.__TAURI_INTERNALS__.invoke;
      const base = await invoke('get_runes');
      const extra = {index:1,title:'Conqueror',primaryPageId:8000,secondaryPageId:8300,keystone:8010,
        primaryRunes:[9111,9104,8299],secondaryRunes:[8304,8347],shards:[5005,5008,5001],play:250,winPct:52,spells:[4,12]};
      const view = {...base,championId:126,championName:'Jayce',position:'top',applied:null,enemyChampionId:0,enemies:[],keystonePresets:[...base.presets,extra]};
      window.__draftBuildCalls=[];
      window.__TAURI_INTERNALS__.invoke=(cmd,args,options)=> {
        if(['get_runes','get_keystone_build','apply_rune_page'].includes(cmd)) window.__draftBuildCalls.push({cmd,args});
        if(cmd==='get_runes') return Promise.resolve(view);
        if(cmd==='full_app_draft') return Promise.resolve({supported:true,allies:[{championId:126,position:'',local:true,locked:true}],enemies:[],bans:[],timer:null});
        if(cmd==='get_other_build') return Promise.resolve(null);
        return invoke(cmd,args,options);
      };
    })()`);
    await nav('Draft', '.cs-build-row');
    await page.text('.cs-list-tabs', 'Builds · Top');
    await page.visible('button[aria-label="Top"][aria-pressed="true"]');
    await page.visible('.cs-inferred-lane[data-tooltip]');
    await page.visible('button[aria-label="More main runes: Choose"]');
    assert.equal(await page.evaluate("window.__draftBuildCalls.some(c=>c.cmd==='get_runes' && c.args?.position==='mid')"), false);
    await page.click('button[aria-label="More main runes: Choose"]');
    await page.text('.fa-picker-option', 'Conqueror');
    await page.click('.fa-picker-option');
    await page.text('.cs-build-selected', 'Conqueror');
    assert.ok(await page.evaluate("window.__draftBuildCalls.some(c=>c.cmd==='get_keystone_build' && c.args?.championId===126 && c.args.position==='top' && c.args.keystone===8010)"));
    assert.equal(await page.evaluate("!!document.querySelector('button[aria-label^=\"More main runes:\"]')"), false);
    await page.click('.cs-apply-bar .fa-primary');
    await page.wait("window.__draftBuildCalls.some(c=>c.cmd==='apply_rune_page' && c.args.selection.keystone===8010)", 'chosen main rune applies');
  });
  await check("Draft planning fits the viewport", async () => {
    await navigate(`${draft}?fixture=planning`, ".cs-pick");
    assert.equal(await page.evaluate("!!document.querySelector('.fa-draft-page :is(.ui-page-header, .cs-preview, .cs-band-foot)')"), false);
    await page.visible('.cs-picks .cs-role-buttons');
    await page.visible('.cs-picks [aria-label^="Rank:"]');
    assert.equal(await page.evaluate("['html', 'body', '.dashboard-main', '.fa-draft-page'].every(selector => { const el = document.querySelector(selector); return el.scrollHeight <= el.clientHeight + 1; })"), true, "only the champion grid may scroll");
    assert.equal(await page.evaluate("!!document.querySelector('.cs-workspace > .cs-planning')"), true, "picks occupy the shared workspace below the draft band");
    await page.evaluate("Promise.all(document.querySelector('.fa-draft-page').getAnimations().map(animation => animation.finished)).then(() => true)");
    draftLayout = await draftGeometry();
    await page.click('.cs-picks button[aria-label="Top"]');
    await page.click('.cs-picks .fa-seg button', 'Recommended');
    await page.wait("document.querySelectorAll('.cs-pick').length === 10", "ten curated recommendations");
    assert.equal(await page.evaluate("[...document.querySelectorAll('.cs-pick-select')].every(button => button.dataset.tooltip.includes(button.getAttribute('aria-description')) && !!button.getAttribute('aria-description'))"), true, "curated reasons are available on hover and to assistive technology");
    assert.equal(await page.evaluate("document.querySelector('.dashboard-main').scrollHeight <= document.querySelector('.dashboard-main').clientHeight + 1"), true, "ten curated cards fit in the scrollable grid");
    assert.equal(await page.evaluate("document.querySelector('.cs-bans').firstElementChild.getAttribute('aria-label')"), 'Banned: Yasuo');
    await page.click('.cs-picks .fa-seg button', 'All');
    await page.wait("document.querySelectorAll('.cs-pick').length > 8", "all champions to load");
    const card = await page.evaluate("(() => { const r = document.querySelector('.cs-pick').getBoundingClientRect(); return { width: r.width, height: r.height }; })()");
    assert.ok(card.width >= 120 && card.height >= card.width * 1.3, `the overflowing All grid keeps portrait cards (${card.width}x${card.height})`);
    assert.equal(await page.evaluate("document.querySelector('.dashboard-main').scrollHeight <= document.querySelector('.dashboard-main').clientHeight + 1"), true);
  });
  await check("Draft shows Lock in only after clicking a champion", async () => {
    await page.type('input[aria-label="Search champion"]', "Aatrox");
    await page.text(".cs-pick strong", "Aatrox");
    assert.equal(await page.evaluate("document.querySelector('.cs-pick-select').getAttribute('aria-pressed')"), "false", "locking must also work before declaring intent");
    const position = await page.evaluate("(() => { const r = document.querySelector('.cs-pick').getBoundingClientRect(); return {x: r.x + r.width / 2, y: r.y + r.height / 3}; })()");
    await page.cdp.send("Input.dispatchMouseEvent", { type: "mouseMoved", ...position });
    assert.equal(await page.evaluate("document.querySelector('.cs-lock-in').disabled"), true, "Lock in stays inactive until a champion is selected");
    await page.click(".cs-pick-select");
    await page.visible(".cs-lock-in:not(:disabled)");
    assert.equal(await page.evaluate("document.querySelector('.cs-lock-in').dataset.tooltip"), "Lock in Aatrox", "the button locks the selected champion");
  });
  await check("Draft one click locks and preserves the complete top layout", async () => {
    await page.click(".cs-lock-in");
    await page.visible(".fa-build-section");
    await page.wait("!document.querySelector('.cs-planning')", "planning panel to close");
    assert.deepEqual(await draftGeometry(), draftLayout, "team cards, bans, timer and the replacement workspace keep their complete bounds");
    await page.visible('.cs-workspace > .fa-build-section');
    assert.equal(await page.evaluate("!!document.querySelector('.cs-lock, .cs-preview')"), false);
    assert.equal(await page.evaluate("document.querySelector('.dashboard-main').scrollHeight <= document.querySelector('.dashboard-main').clientHeight + 1"), true);
  });
  for (const fixture of ["hover", "locked", "locked-blind"]) {
    await check(`Draft ${fixture} fixture`, async () => {
      await navigate(`${draft}?fixture=${fixture}`, fixture === "hover" ? ".cs-pick" : ".fa-build-section");
      if (fixture !== "hover") await page.text(".cs-toolbar h2", "Aatrox");
      await page.visible('[aria-label="Your team\'s picks"]');
      await page.visible('[aria-label="Enemy team\'s picks"]');
      if (fixture === "locked") {
        await page.visible('.cs-pro-pages');
        await page.click('.cs-pro-pages button', 'Next');
        await page.text('.cs-build-row.is-pro', 'Pro player D');
        await page.text('.cs-build-row.is-pro', 'Pro player F');
        await page.click('.cs-pro-pages button', 'Previous');
        await page.text('.cs-build-row.is-pro', 'Pro player A');
      }
      if (fixture === "locked-blind") await page.wait("!document.querySelector('.cs-toolbar h2 small')", "blind draft has no revealed opponent");
    });
  }
  await check("Draft matchup summary keeps builds visible and reuses live guide tips", async () => {
    await navigate(`${draft}?fixture=locked-guide`, '.cs-matchup-guide');
    await page.text('.cs-toolbar h2', 'Ahri');
    await page.text('.cs-toolbar h2', 'Zed');
    await page.text('.cs-matchup-guide > summary', 'Skill matchup');
    await page.text('.cs-matchup-guide > summary', '5W – 20L');
    const layout = await page.evaluate(`(() => {
      const guide = document.querySelector('.cs-matchup-guide');
      const rune = document.querySelector('.fa-build-section .fa-rune-row');
      return { open: guide.open, height: guide.getBoundingClientRect().height, runeTop: rune?.getBoundingClientRect().top, viewport: innerHeight };
    })()`);
    assert.equal(layout.open, false);
    assert.ok(layout.height < 90 && layout.runeTop < layout.viewport - 24, JSON.stringify(layout));
    await screenshot('draft-matchup-guide-collapsed');
    await page.click('.cs-matchup-guide > summary');
    await page.visible('.cs-matchup-guide .mg-summary');
    assert.equal(await page.evaluate("document.querySelectorAll('.cs-matchup-guide .mg-tips > li').length"), 3);
    await page.text('.cs-matchup-guide .mg-author-heading strong', 'Peng04');
    await page.text('.cs-matchup-guide .mg-live-cooldown', '20/18.5/17/15.5/14s');
    assert.equal(await page.evaluate("!!document.querySelector('.cs-matchup-guide .mg-deck, .cs-matchup-guide .mg-sources-trigger, .cs-matchup-guide .mg-visual')"), false);
    await screenshot('draft-matchup-guide-expanded');
    await page.click('.cs-guide-footer button', 'Open full guide');
    await page.text('.mg-workspace-tabs [aria-selected="true"]', 'Guide');
    await page.text('.mg-context strong', 'Ahri vs Zed');
    await page.text('.mg-context', 'Mid');
  });
  await check("Draft without a guide shows only an available record", async () => {
    await navigate(`${draft}?fixture=locked&record`, '.cs-guide-record-only');
    await page.text('.cs-guide-record-only', '5W – 20L');
    assert.equal(await page.evaluate("!!document.querySelector('.cs-matchup-guide, .cs-guide-footer')"), false);
    await navigate(`${draft}?fixture=locked-blind`, '.fa-build-section');
    assert.equal(await page.evaluate("!!document.querySelector('.cs-matchup-guide, .cs-guide-record-only')"), false);
  });
  await check("Open full guide navigates the real app to this pair and Guide tab", async () => {
    await navigate(`${fullApp}?draft-guide`, 'nav[aria-label="Swapper pages"]');
    await nav('Draft', '.cs-matchup-guide');
    const runeTop = await page.evaluate("document.querySelector('.fa-build-section .fa-rune-row').getBoundingClientRect().top");
    assert.ok(runeTop < 820 - 24, `full-app runes must stay in view: ${runeTop}`);
    await screenshot('draft-matchup-guide-shell');
    await page.click('.cs-matchup-guide > summary');
    await page.click('.cs-guide-footer button', 'Open full guide');
    await page.text('nav[aria-label="Swapper pages"] [aria-current="page"]', 'Matchups');
    await page.text('.mg-workspace-tabs [aria-selected="true"]', 'Guide');
    await page.text('.mg-context strong', 'Ahri vs Zed');
    await page.text('.mg-context', 'Mid');
    await screenshot('draft-open-full-guide');
    assert.equal(await page.evaluate("!!document.querySelector('.mg-deck .ui-notice.is-warn')"), false, 'a matchup-specific guide has no general-guide banner');
    await page.wait("document.querySelectorAll('.mg-context button[data-slot=\"button\"]').length === 2", 'guide sources and VOD actions to load');
    const actions = await page.evaluate(`Array.from(document.querySelectorAll('.mg-context button[data-slot="button"]'), button => {
      const style = getComputedStyle(button);
      return { variant: button.dataset.variant, height: button.getBoundingClientRect().height, radius: style.borderRadius,
        padding: style.padding, background: style.backgroundColor, icon: button.querySelector('svg')?.getAttribute('aria-hidden'), count: button.querySelector('b')?.textContent };
    })`);
    assert.deepEqual(actions.map(action => action.variant), ['secondary', 'secondary']);
    assert.equal(actions[0].height, actions[1].height);
    assert.equal(actions[0].radius, actions[1].radius);
    assert.equal(actions[0].padding, actions[1].padding);
    assert.ok(actions.every(action => action.icon === 'true' && Number(action.count) > 0));
    await page.click('.mg-sources-trigger');
    await page.visible('[aria-label="Guide sources"]');
    await cdp.send('Input.dispatchKeyEvent', { type: 'keyDown', key: 'Escape', code: 'Escape', windowsVirtualKeyCode: 27 });
    await cdp.send('Input.dispatchKeyEvent', { type: 'keyUp', key: 'Escape', code: 'Escape', windowsVirtualKeyCode: 27 });
  });
  await check("General guide fallback renders in Draft and the Matchups carousel", async () => {
    await navigate(`${fullApp}?draft-guide`, 'nav[aria-label="Swapper pages"]');
    await page.evaluate(`(() => {
      const invoke = window.__TAURI_INTERNALS__.invoke;
      const tip = { scope: "champion", text: "General Ahri advice." };
      const guide = { champion: 103, lane: "mid", patch: "26.20", reviewed: true,
        keyTips: [tip, tip, tip], itemization: [], sources: [],
        sections: Object.fromEntries(["overview", "early-lane", "mid-lane", "win-condition"].map(id => [id, { tips: [tip] }])) };
      window.__TAURI_INTERNALS__.invoke = (cmd, args, options) => cmd === "get_matchup_guide"
        ? Promise.resolve({ guide, record: null }) : invoke(cmd, args, options);
    })()`);
    await nav('Draft', '.cs-matchup-guide');
    await page.text('.cs-matchup-guide > summary', 'General Ahri guide · no Zed matchup guide yet');
    assert.equal(await page.evaluate("!!document.querySelector('.cs-guide-difficulty, .cs-guide-outlook')"), false);
    await page.click('.cs-matchup-guide > summary');
    await page.text('.cs-guide-reading', 'General Ahri advice.');
    assert.equal(await page.evaluate("!!document.querySelector('.cs-guide-reading .mg-summary-outlook')"), false);
    await page.click('.cs-guide-footer button', 'Open full guide');
    await page.text('.mg-deck .ui-notice[role="status"]', "No Ahri vs Zed guide yet. Showing the general Ahri guide, so tips aren't specific to this matchup.");
    assert.equal(await page.evaluate("document.querySelector('.mg-deck .ui-notice svg').getAttribute('aria-hidden')"), 'true');
    await page.text('.mg-slide:not([hidden])', 'General Ahri advice.');
    assert.equal(await page.evaluate("!!document.querySelector('.mg-summary-outlook, .mg-context .mg-difficulty')"), false);
    await page.click('.mg-tabs [role="tab"]', 'Win condition');
    await page.text('.mg-slide:not([hidden])', 'General Ahri advice.');
    await page.evaluate("document.querySelector('button[aria-label=\"Collapse guide\"]').focus()");
    await cdp.send('Input.dispatchKeyEvent', { type: 'keyDown', key: 'Enter', code: 'Enter', text: '\r', windowsVirtualKeyCode: 13 });
    await cdp.send('Input.dispatchKeyEvent', { type: 'keyUp', key: 'Enter', code: 'Enter', windowsVirtualKeyCode: 13 });
    await page.visible('button[aria-label="Expand guide"][aria-expanded="false"]');
    assert.ok(await page.evaluate("document.querySelector('.mg-deck-content').hidden"));
    assert.ok(await page.evaluate("document.querySelector('.mg-deck').getBoundingClientRect().height >= document.querySelector('.mg-card-header').getBoundingClientRect().height"), 'collapsed card keeps its header height');
    assert.equal(await page.evaluate("localStorage.getItem('swapper.guideHidden')"), '1');
    await nav('Draft', '.cs-matchup-guide');
    await page.click('.cs-guide-footer button', 'Open full guide');
    await page.visible('button[aria-label="Expand guide"][aria-expanded="false"]');
    await page.click('button[aria-label="Expand guide"]');
    await page.visible('.mg-tabs');
  });
  await check("Guide tips inline listed items and rune names without duplicate reference chips", async () => {
    await navigate(`${fullApp}?draft-guide`, 'nav[aria-label="Swapper pages"]');
    await page.evaluate(`(() => {
      const invoke = window.__TAURI_INTERNALS__.invoke;
      const tip = { scope: "champion", text: "Take Conqueror or Second Wind. Open Black Cleaver with Triumph, Alacrity and Haste; ability haste and Eclipse stay plain.", itemIds: [3071] };
      const guide = { champion: 103, lane: "mid", patch: "26.20", reviewed: true,
        keyTips: [tip, tip, tip], itemization: [], sources: [],
        sections: Object.fromEntries(["overview", "early-lane", "mid-lane", "win-condition"].map(id => [id, { tips: [tip] }])) };
      window.__TAURI_INTERNALS__.invoke = (cmd, args, options) => cmd === "get_matchup_guide"
        ? Promise.resolve({ guide, record: null }) : invoke(cmd, args, options);
    })()`);
    await nav('Draft', '.cs-matchup-guide');
    await page.click('.cs-matchup-guide > summary');
    await page.wait("document.querySelectorAll('.cs-guide-reading .mg-tips li:first-child .mg-inline-reference img').length === 6", 'inline item, utility runes and short aliases');
    const inline = await page.evaluate(`(() => {
      const tip = document.querySelector('.cs-guide-reading .mg-tips li');
      return { names: [...tip.querySelectorAll('.mg-inline-reference')].map(el => el.textContent),
        sizes: [...tip.querySelectorAll('.mg-inline-reference img')].map(el => el.getBoundingClientRect().width / parseFloat(getComputedStyle(el.parentElement).fontSize)),
        chips: tip.querySelectorAll('.mg-references .ui-slot.is-item').length,
        rows: tip.querySelectorAll('.mg-references').length };
    })()`);
    assert.deepEqual(inline.names, ['Conqueror', 'Second Wind', 'Black Cleaver', 'Triumph', 'Alacrity', 'Haste']);
    assert.ok(inline.sizes.every(size => Math.abs(size - 1.2) < .01), JSON.stringify(inline.sizes));
    assert.equal(inline.chips, 0);
    assert.equal(inline.rows, 0);
  });
  await (await import('./guide-layout.mjs')).guideLayoutChecks({ check, navigate, fullApp, page, nav, cdp: page.cdp });
  await check("Draft lock busy state and errors", async () => {
    await navigate(`${draft}?fixture=planning&lock-delay&lock-fail`, '.cs-pick');
    await page.type('input[aria-label="Search champion"]', 'Aatrox');
    await page.visible('.cs-pick-select');
    await page.click('.cs-pick-select');
    await page.visible('.cs-lock-in:not(:disabled)');
    await page.evaluate("document.querySelector('.cs-lock-in').focus()");
    await page.wait("document.activeElement.matches('.cs-lock-in')", 'Lock in is keyboard focusable');
    await page.evaluate("Promise.all(document.querySelector('.fa-draft-page').getAnimations({ subtree: true }).filter(animation => animation.effect.getTiming().iterations !== Infinity).map(animation => animation.finished)).then(() => true)");
    const beforeFailure = await draftGeometry();
    await page.click('.cs-lock-in');
    await page.visible('.cs-lock-in:disabled[aria-busy="true"]');
    await page.text('.cs-picks .ui-notice', 'Lock in failed. Try again.');
    assert.deepEqual(await draftGeometry(), beforeFailure, "an error stays in the workspace without moving the top section");
    assert.equal(await page.evaluate("document.body.dataset.lockCalls"), '1');
    await page.visible('.cs-lock-in:not(:disabled)');
    await page.evaluate("history.replaceState(null, '', location.href.replace('&lock-fail', ''))");
    await page.click('.cs-lock-in');
    await page.visible('.fa-build-section');
  });
  await check("Draft hides lock outside your pick turn or availability", async () => {
    for (const query of ['turn=waiting', 'turn=intent']) {
      await navigate(`${draft}?fixture=planning&${query}`, '.cs-pick');
      await page.type('input[aria-label="Search champion"]', 'Aatrox');
      await page.visible('.cs-pick-select');
      await page.click('.cs-pick-select'); await page.visible('.cs-pick.is-hovered');
      assert.equal(await page.evaluate("document.querySelector('.cs-lock-in').disabled"), true, query);
    }
  });
  await check("Draft bans the selected champion on your ban turn", async () => {
    await navigate(`${draft}?fixture=planning&turn=ban&unavailable`, '.cs-pick');
    await page.text('.cs-lock-in', 'Ban');
    assert.equal(await page.evaluate("document.querySelector('.cs-lock-in').disabled"), true, 'nothing selected to ban yet');
    await page.type('input[aria-label="Search champion"]', 'Aatrox');
    await page.click('.cs-pick-select');
    await page.visible('.cs-pick.is-hovered');
    await page.wait("!document.querySelector('.cs-lock-in').disabled", 'the Ban button for a champion you do not own');
    await page.click('.cs-lock-in');
    await page.wait("document.body.dataset.banned === '266'", 'the ban of the selected champion');
  });
  await check("Draft hides champions the player can't pick", async () => {
    await navigate(`${draft}?fixture=planning&unavailable&turn=waiting`, '.cs-pick');
    await page.click('.cs-picks .fa-seg button', 'All');
    await page.wait("document.querySelectorAll('.cs-pick strong').length > 3", 'the champion list');
    assert.equal(await page.evaluate("[...document.querySelectorAll('.cs-pick strong')].some(n => n.textContent === 'Aatrox')"), false, 'an unowned champion is not offered on All');
    await page.type('input[aria-label="Search champion"]', 'Aatrox');
    await page.wait("!document.querySelector('.cs-pick-select')", 'search to skip the unowned champion');
  });
  await check("Draft role buttons override the assigned role, also on All", async () => {
    await navigate(`${draft}?fixture=planning`, '.cs-pick');
    assert.equal(await page.evaluate("document.querySelectorAll('.cs-role-buttons button').length"), 5);
    const assigned = await page.evaluate("document.querySelector('.cs-role-buttons [aria-pressed=\"true\"]').getAttribute('aria-label')");
    const target = assigned === 'Jungle' ? 'Support' : 'Jungle';
    await page.click('.cs-picks .fa-seg button', 'All');
    await page.click(`.cs-role-buttons button[aria-label="${target}"]`);
    await page.wait(`document.querySelector('.cs-role-buttons [aria-pressed="true"]')?.getAttribute('aria-label') === '${target}'`, 'the picked role to stick');
  });
  await check("Counters offer each likely laner when ranked hides enemy roles", async () => {
    await navigate(`${draft}?fixture=hidden-roles`, '.cs-pick');
    await page.click('.cs-picks-tools .fa-seg button', 'Counters');
    await page.wait("document.querySelectorAll('.cs-laner-pick button').length === 2", 'both revealed enemies as laner choices');
    await page.visible('.cs-pick-grid .cs-pick');
    await page.click('.cs-laner-pick button:not([aria-pressed="true"])');
    await page.wait("document.querySelectorAll('.cs-laner-pick [aria-pressed=\"true\"]').length === 1", 'the other laner to be chosen');
    await page.visible('.cs-pick-grid .cs-pick');
  });
  await check("Rune tree menus show each tree's colour without a heading", async () => {
    await navigate(`${draft}?fixture=locked`, '.fa-build-section .fa-rune-tree .fa-picker');
    assert.equal(await page.evaluate("!!document.querySelector('.fa-rune-tree .fa-picker.is-accented')?.style.getPropertyValue('--accent')"), true, 'the chosen tree shows its colour');
    await page.click('.fa-build-section .fa-rune-tree .fa-picker');
    await page.wait("document.querySelectorAll('.fa-picker-popover.fa-tree-picker .fa-picker-option.is-accented').length >= 3", 'coloured tree options');
    assert.equal(await page.evaluate("!!document.querySelector('.fa-picker-popover.fa-tree-picker .fa-picker-label')"), false, 'no Primary tree heading');
  });
  await check("Custom runes save as a champion preset, reopen and delete", async () => {
    await navigate(`${draft}?fixture=locked`, '.fa-build-section .fa-rune-row button');
    await page.click('.fa-build-section .fa-rune-row button:not(.is-chosen)');
    await page.click('.cs-rune-actions button', 'Save as preset');
    await page.type('.cs-preset-save input', 'Lane bully');
    await page.click('.cs-preset-save button', 'Save');
    await page.text('.fa-rune-area h4', 'Lane bully');
    await page.click('.cs-list-tabs button:nth-child(2)');
    await page.wait("document.querySelector('.cs-list-tabs + .cs-build-entries > .cs-build-row')?.getAttribute('aria-pressed') === 'true'", 'the saved preset selected in the presets tab');
    await page.click('.cs-list-tabs button:first-child');
    await page.click('.cs-list-tabs ~ div.cs-build-entries > .cs-build-row:first-child');
    await page.wait("document.querySelector('.fa-rune-area h4')?.textContent === 'Runes'", 'the stats runes back');
    await page.click('.fa-build-section .fa-rune-row button:not(.is-chosen):nth-child(2)');
    await page.click('.cs-rune-actions button', 'Save as preset');
    await page.wait("document.querySelector('.cs-preset-save input')?.value === ''", 'an empty name for the second preset');
    await page.type('.cs-preset-save input', 'Safe');
    await page.click('.cs-preset-save button', 'Save');
    await page.click('.cs-list-tabs button:nth-child(2)');
    await page.wait("document.querySelectorAll('.cs-list-tabs + .cs-build-entries > .cs-build-row').length === 2", 'two presets');
    await page.click('.cs-list-tabs + .cs-build-entries > .cs-build-row:first-child');
    await page.text('.fa-rune-area h4', 'Lane bully');
    await page.click('.fa-build-section .fa-rune-row button:not(.is-chosen):nth-child(3)');
    await page.click('.cs-rune-actions button', 'Update Lane bully');
    await page.text('.fa-rune-area h4', 'Lane bully');
    await page.wait("document.querySelectorAll('.cs-list-tabs + .cs-build-entries > .cs-build-row').length === 2", 'the update replaces in place');
    await page.click('.cs-preset-delete');
    await page.wait("document.querySelectorAll('.cs-list-tabs + .cs-build-entries > .cs-build-row').length === 1", 'one preset left');
    await page.click('.cs-preset-delete');
    await page.text('.cs-build-list > p', 'No presets');
  });
  await check("A pro build can lend only its runes or only its items", async () => {
    await navigate(`${draft}?fixture=locked`, '.cs-build-row.is-pro');
    await page.click('.cs-build-row.is-pro');
    await page.wait("document.querySelector('.cs-build-selected strong')?.textContent.endsWith('’s build')", 'the whole pro build');
    await page.click('.cs-pro-part button', 'Runes only');
    await page.wait("document.querySelector('.cs-build-selected strong')?.textContent.endsWith('’s runes')", 'pro runes only');
    await page.text('.cs-build-selected small', 'with Most played items');
    await page.click('.cs-pro-part button', 'Items only');
    await page.text('.cs-build-selected small', 'with Most played runes');
  });
  await check("Hovering a draft card shows that champion's abilities", async () => {
    await navigate(`${draft}?fixture=locked`, '.cs-band .cs-slot:not(.is-empty)');
    assert.equal(await page.evaluate("document.querySelector('.cd-sheet')"), null, "the cooldown strip below the build is gone");
    const box = await page.evaluate("(() => { const r = document.querySelector('.cs-band .cs-slot:not(.is-empty)').getBoundingClientRect(); return { x: r.left + r.width / 2, y: r.top + r.height / 2 }; })()");
    await page.cdp.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: box.x, y: box.y });
    await page.wait("document.querySelectorAll('.fa-ability-card.is-champion section').length === 5", 'five ability sections');
    await page.text(".fa-ability-card.is-champion", "Passive");
    await page.cdp.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: 2, y: 2 });
    await page.wait("!document.querySelector('.fa-ability-card.is-champion')", 'the card to close');
  });
  await check("A just-ended game shows on the dashboard before match history has it", async () => {
    await navigate(fullApp + "?ended", ".match-row");
    await page.wait("document.querySelector('.match-row .match-kda')?.textContent.includes('7 / 2 / 9')", 'the just-ended game on top');
    await page.text(".match-row", "Victory");
    await page.click(".match-row .match-champ");
    await page.visible('#md-tab-Overview');
  });
  await check("Tray flyout browser preview", async () => {
    await navigate("/index.html", ".shell .main-panel");
    await page.text("h1", "Accounts");
    await page.visible(".empty-state");
    await page.visible('button[aria-label="Settings"]');
  });
  await check("Closed client offers Open League in the tray", async () => {
    await navigate('/src/flyout-preview.html?client=closed', '.account-row.is-active');
    await page.text('.account-row.is-active small', 'Open League');
    await page.evaluate("window.__TAURI_INTERNALS__.invoke('plugin:event|emit', { event: 'client_health', payload: 'ok' })");
    await page.text('.account-row.is-active small', 'Restart client');
    await page.evaluate("window.__TAURI_INTERNALS__.invoke('plugin:event|emit', { event: 'client_health', payload: 'closed' })");
    await page.text('.account-row.is-active small', 'Open League');
  });
  if (existsSync(path.join(root, setup))) {
    await check("Setup next back and finish", async () => {
      await navigate(setup, '.ob-card [role="radio"]');
      await page.text("#ob-title", "How you use Swapper");
      await page.click(".ob-card button", "Next");
      await page.text("#ob-title", "In game");
      await page.click(".ob-card button", "Back");
      await page.text("#ob-title", "How you use Swapper");
      await page.click(".ob-card button", "Next");
      await page.click(".ob-card button", "Next");
      await page.text("#ob-title", "Extras");
      await audit();
      await screenshot("setup-extras");
      await page.click(".ob-card button", "Done");
      await page.wait("document.body.dataset.closed === '1'", "setup close callback");
    });
    await check("Setup skip asks and can be cancelled", async () => {
      await navigate(setup, '.ob-card [role="radio"]');
      await page.click(".ob-card button", "Skip setup");
      await page.visible('[role="alertdialog"][aria-label="Skip setup"]');
      assert.equal(await page.evaluate("document.body.dataset.closed"), undefined);
      await page.click(".ob-confirm button", "Keep going");
      await page.wait("!document.querySelector('.ob-confirm')", "skip confirmation to close");
      await page.click(".ob-card button", "Skip setup");
      await page.click(".ob-confirm button", "Skip setup");
      await page.wait("document.body.dataset.closed === '1'", "confirmed skip close callback");
    });
  } else console.log("SKIP setup preview: file not present");
  await check("No late page or console errors", () => page.settle());
}

try { await main(); assert.ok(results.length > 0, "No browser scenarios matched the requested filter."); }
catch (error) {
  results.push({ name: current, passed: false, failure: error.stack });
  console.error(`FAIL ${current}: ${error.stack}`);
} finally {
  try { await cleanup(); }
  catch (error) { results.push({ name: "cleanup", passed: false, failure: error.message }); console.error(`FAIL cleanup: ${error.message}`); }
  clearTimeout(watchdog);
}
const failed = results.filter(result => !result.passed).length;
const seconds = ((performance.now() - started) / 1000).toFixed(2);
console.log(`\n${results.length - failed} passed, ${failed} failed | ${seconds}s (including startup and cleanup)`);
for (const [reason, count] of allowed) console.log(`ALLOW ${count}x: ${reason}`);
if (screenshots) {
  await writeFile(path.join(screenshots, "report.json"), JSON.stringify({ seconds: Number(seconds), results, issues, allowances: [...allowed] }, null, 2));
  console.log(`Screenshots and report: ${screenshots}`);
}
process.exitCode = failed ? 1 : 0;
