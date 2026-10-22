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
const unknown = process.argv.slice(2).filter(arg => arg !== "--screenshots");
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
  console.error(`FAIL suite exceeded 55 seconds during ${current}`);
  void cleanup().finally(() => process.exit(1));
}, 55_000);

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
  await page.visible(selector);
}

async function shell() {
  await page.visible('nav[aria-label="Swapper pages"]');
  for (const label of ["Dashboard", "Tierlist", "Matchups", "Draft", "Accounts", "Settings"]) {
    await page.text('nav[aria-label="Swapper pages"] button', label);
  }
  await page.visible(".ui-page-header h1");
}

async function nav(label, selector) {
  await page.click('nav[aria-label="Swapper pages"] button', label);
  await page.text('nav[aria-label="Swapper pages"] [aria-current="page"]', label);
  await page.text(".ui-page-header h1", label);
  await page.visible(selector);
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
      if (url.origin === origin) return cdp.send("Fetch.continueRequest", { requestId });
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
  if (capture) {
    await screenshot("dashboard-evidence", 1440, 900);
    await cdp.send("Emulation.setDeviceMetricsOverride", { width: 1280, height: 820, deviceScaleFactor: 1, mobile: false });
  }
  await check("Dashboard match opens and returns", async () => {
    await page.click(".match-champ");
    await page.visible(".md-page .md-team");
    await page.text(".ui-page-header h1", "Victory");
    assert.equal(await page.evaluate("document.querySelectorAll('.md-player:not([aria-hidden])').length"), 10);
    await audit();
    await screenshot("match-detail");
    // ---- matchdetail: comparison, loadout, searchable stats and timeline graph ----
    assert.equal(await page.evaluate("document.querySelectorAll('.md-compare-row').length"), 10);
    assert.equal(await page.evaluate("document.querySelectorAll('.md-fact').length"), 3);
    assert.equal(await page.evaluate("document.querySelector('.md-tower-reading span.is-blue').textContent"), 'Your team');
    assert.ok(await page.evaluate("document.querySelector('.md-share-ring').getAttribute('aria-label').includes('damage to champions')"));
    await page.click('#md-tab-Loadout');
    await page.visible('.md-runes');
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
    for (const [width, height] of [[1440, 900], [2560, 1440], [1280, 820]]) {
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
    await page.click('#md-tab-Scoreboard');
    // ---- end matchdetail tabs ----
    await page.click('.ui-crumbs button', "Dashboard");
    await page.visible(".match-row.is-last");
  });
  // ---- matchdetail: unavailable timeline and absent stats never become fake zeroes ----
  await check('Match stats with unavailable fields and timeline', async () => {
    await navigate(fullApp + '?no-timeline&detail-missing', '.match-row');
    await page.click('.match-champ');
    await page.visible('.md-team');
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
    await navigate(fullApp, '.match-row');
  });
  await check('Match graphs: dense lanes, kill map, player filters and keyboard', async () => {
    await navigate(fullApp + '?timeline-long', '.match-row');
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
    await page.click('.md-event-filters button', 'All');
    await page.click('.md-event-expand button');
    assert.equal(await page.evaluate("document.querySelectorAll('.md-event-list button').length"), 90);
    await page.click('.md-event-expand button');
    assert.equal(await page.evaluate("document.querySelectorAll('.md-event-list button').length"), 12);
    await navigate(fullApp + '?timeline-long&timeline-no-positions', '.match-row');
    await page.click('.match-champ');
    await page.click('#md-tab-Graphs');
    await page.visible('.md-event-lane');
    assert.equal(await page.evaluate("document.querySelector('.md-kill-map')"), null);
    assert.equal(await page.evaluate("document.querySelectorAll('.md-lane-mark.is-kill').length"), 24);
    await navigate(fullApp, '.match-row');
  });
  // ---- end matchdetail missing data ----
  await check("Sidebar Tierlist", () => nav("Tierlist", ".tier-table tbody tr"));
  await check("Tierlist champion overview and build", async () => {
    await page.click(".tier-champ", "Aatrox");
    await page.text(".champion-hero h2", "Aatrox");
    await page.visible(".fa-build-section");
    await audit();
    await screenshot("champion-overview");
    await page.click(".back-to-list");
    await page.visible(".tier-table tbody tr");
  });
  await check("Sidebar Matchups", () => nav("Matchups", ".fa-matchup-empty"));
  await check("Matchups chooses both champions", async () => {
    await page.click('button[aria-label^="Your champion:"]');
    await page.click(".fa-picker-option", "Aatrox");
    await page.text(".champion-hero h2", "Aatrox");
    await page.click('button[aria-label^="Opponent:"]');
    await page.click(".fa-picker-option", "Darius");
    await page.text('[aria-label="Matchup"]', "Darius");
    await page.text('[aria-label="Matchup"]', "47.6");
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
    const code = { Enter: 13, Escape: 27, ArrowDown: 40 }[key] ?? 0;
    await page.cdp.send('Input.dispatchKeyEvent', { type: 'keyDown', key, modifiers, windowsVirtualKeyCode: code });
    await page.cdp.send('Input.dispatchKeyEvent', { type: 'keyUp', key, modifiers, windowsVirtualKeyCode: code });
  }
  const searchInput = '.palette input';
  await check('Palette icons, groups, focus and empty results', async () => {
    await page.evaluate("document.querySelector('.title-search').focus()");
    await key('k', 2);
    await page.visible('.palette');
    await page.text('.palette h3', 'Pages');
    assert.equal(await page.evaluate("getComputedStyle(document.querySelector('.palette input')).outlineStyle"), 'none');
    assert.ok(await page.evaluate("document.querySelector('.palette-nav-icon svg') !== null"));
    await page.type(searchInput, 'nothing-will-match');
    await page.text('.palette-empty', 'No results found');
    assert.equal(await page.evaluate("document.querySelector('.palette input').getAttribute('aria-activedescendant')"), null);
    await key('ArrowDown');
    await key('Escape');
    await page.visible('.title-search:focus');
  });
  await check('Palette champion selection and local recents', async () => {
    await page.click('.title-search');
    await page.type(searchInput, 'Ahri');
    await page.visible('.palette-option .champion-icon');
    await key('Enter');
    await page.text('.champion-hero h2', 'Ahri');
    await page.click('.title-search');
    await page.text('.palette h3', 'Recent');
    await page.click('.palette-hint button', 'Clear recent');
    assert.equal(await page.evaluate("document.querySelector('[role=group][aria-label=Recent]')"), null);
    await key('Escape');
  });
  await check('Palette player profile, match details and back', async () => {
    await nav('Dashboard', '.match-row');
    await page.click('.title-search');
    await page.type(searchInput, 'Player 1#DEMO');
    await page.visible('.palette-option .search-profile-icon');
    await page.text('.palette-player-copy', 'Played with you');
    await key('Enter');
    await page.text('.player-identity h2', 'Player 1#DEMO');
    assert.equal(await page.evaluate("document.querySelector('.palette')"), null);
    await page.visible('.player-games .match-row');
    await page.text('.player-ranks', 'Solo/Duo');
    await page.text('.player-ranks', 'Flex');
    await page.click('.player-games .match-champ');
    await page.visible('.md-team');
    await page.click('.ui-crumbs button', 'Player profile');
    await page.visible('.player-identity');
    await page.click('.player-back');
    await page.text('.ui-page-header h1', 'Dashboard');
  });
  await check('Palette no-client saved players and keyboard across groups', async () => {
    await navigate(`${fullApp}?stale`, '.match-row');
    await page.click('.title-search');
    await page.type(searchInput, 'Player 5');
    await page.text('.palette-player-copy', 'Played against you');
    await page.text('.palette-player-status', 'Open League Client');
    await key('Enter');
    await page.text('.player-page .ui-notice', 'Open League Client');
    await page.visible('.player-games .match-row');
    await page.click('.title-search');
    const recentCount = await page.evaluate("document.querySelectorAll('.palette [role=group][aria-label=Recent] [role=option]').length");
    for (let i = 0; i < recentCount; i++) await key('ArrowDown');
    await key('Enter');
    await page.text('.ui-page-header h1', 'Dashboard');
    await page.click('.title-search');
    await page.click('.palette-hint button', 'Clear recent');
    await key('Escape');
    await navigate(fullApp, '.match-row');
  });
  await check('Player partial data keeps games and offers retry', async () => {
    await navigate(`${fullApp}?search-partial`, '.match-row');
    await page.click('.title-search');
    await page.type(searchInput, 'Player 1#DEMO');
    await page.visible('.palette-player-copy');
    await key('Enter');
    await page.text('.player-page .ui-notice', 'Some profile data could not be loaded');
    await page.visible('.player-games .match-row');
    await page.visible('.player-page .ui-notice button:not(:disabled)');
  });
  await check('Player lookup failure keeps saved rows and offers retry', async () => {
    await navigate(`${fullApp}?search-error`, '.match-row');
    await page.click('.title-search');
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
        await nav("Live game", ".lg-team");
        assert.equal(await page.evaluate("document.querySelectorAll('.lg-row').length"), 10);
      }
    });
  }

  await check("Draft planning fixture", async () => {
    await navigate(`${draft}?fixture=planning`, ".cs-pick");
    await page.text(".ui-page-header h1", "Draft");
    await page.visible('.cs-lock button:disabled');
    await page.text(".cs-preview h3", "Choose a champion");
  });
  await check("Draft planning to hover", async () => {
    await page.type('input[aria-label="Search champion"]', "Aatrox");
    await page.text(".cs-pick strong", "Aatrox");
    await page.click(".cs-pick");
    await page.text(".cs-preview h3", "Aatrox");
    await page.visible(".cs-preview-row");
  });
  await check("Draft confirms lock and shows build", async () => {
    await page.click(".cs-lock button", "Lock in");
    await page.text(".cs-lock", "This can’t be undone.");
    await page.click(".cs-lock button", "Confirm");
    await page.text(".fa-status-pill", "Locked in");
    await page.visible(".fa-build-section");
    await page.wait("!document.querySelector('.cs-planning')", "planning panel to close");
  });
  for (const fixture of ["hover", "locked", "locked-blind"]) {
    await check(`Draft ${fixture} fixture`, async () => {
      await navigate(`${draft}?fixture=${fixture}`, fixture === "hover" ? ".cs-preview-row" : ".fa-build-section");
      await page.text(".ui-page-header h1", "Draft");
      await page.text(".cs-toolbar h2", "Aatrox");
      await page.visible('[aria-label="Your team\'s picks"]');
      await page.visible('[aria-label="Enemy team\'s picks"]');
      if (fixture === "locked-blind") await page.wait("!document.querySelector('.cs-toolbar h2 small')", "blind draft has no revealed opponent");
    });
  }
  await check("Tray flyout browser preview", async () => {
    await navigate("/index.html", ".shell .main-panel");
    await page.text("h1", "Accounts");
    await page.visible(".empty-state");
    await page.visible('button[aria-label="Settings"]');
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

try { await main(); }
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
