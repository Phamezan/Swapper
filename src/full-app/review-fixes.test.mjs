import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import ts from "typescript";
import * as runeTypes from "../runes/types.ts";
import * as model from "./champ-select-model.ts";
import * as buildLogic from "./buildLogic.ts";
import * as championsModel from "./championsModel.ts";
import * as format from "./ui/format.ts";
import * as cooldownModel from "./cooldownModel.ts";
import * as proBuilds from "./champ-select-pro.ts";
import * as runesPanelModel from "../runes/runesPanelModel.ts";
import { fixtureView, fixtureDraft } from "./champ-select-fixtures.ts";

// Execute the actual components with deterministic hooks and command/event boundaries.
function harness(file, exportName, commands = {}, visibility = "visible", environment = {}) {
  const states = [], effects = [], memos = [], handlers = new Map(), timers = new Map();
  const calls = [];
  let cursor = 0, pending = [], result, props, nextTimer = 0;
  const changed = (a, b) => !a || a.length !== b.length || a.some((v, i) => !Object.is(v, b[i]));
  const hooks = {
    useState(initial) {
      const i = cursor++;
      if (!(i in states)) states[i] = typeof initial === "function" ? initial() : initial;
      return [states[i], value => { states[i] = typeof value === "function" ? value(states[i]) : value; }];
    },
    useRef(initial) { return hooks.useState({ current: initial })[0]; },
    useEffect(fn, deps) {
      const i = cursor++;
      if (environment.effects === false) return;
      if (changed(effects[i]?.deps, deps)) pending.push(() => {
        effects[i]?.cleanup?.(); effects[i] = { deps, cleanup: fn() };
      });
    },
    useMemo(fn, deps) {
      const i = cursor++;
      if (changed(memos[i]?.deps, deps)) memos[i] = { deps, value: fn() };
      return memos[i].value;
    },
  };
  const document = {
    visibilityState: visibility,
    addEventListener(name, fn) { handlers.set(name, fn); },
    removeEventListener(name) { handlers.delete(name); },
  };
  const window = {
    setInterval(fn) { const id = ++nextTimer; timers.set(id, fn); return id; },
    clearInterval(id) { timers.delete(id); },
    ...environment.window,
  };
  const invoke = async (name, args) => {
    calls.push({ name, args });
    return typeof commands[name] === "function" ? commands[name](args) : commands[name] ?? null;
  };
  const jsx = (type, props) => ({ type, props });
  const modules = {
    react: hooks, "react/jsx-runtime": { jsx, jsxs: jsx, Fragment: "fragment" },
    "@tauri-apps/api/core": { invoke, isTauri: () => true },
    "./AmbientArt": { useAmbientChampion: () => {} },
    "./Picker": { ROLES: [["top", "Top"], ["jungle", "Jungle"], ["mid", "Mid"], ["adc", "Bot"], ["support", "Support"]] },
    "./cooldownModel": cooldownModel,
    "radix-ui": { HoverCard: { Root: "hover-root", Trigger: "hover-trigger", Portal: "hover-portal", Content: "hover-content" } },
    "@tauri-apps/api/event": { listen: async (name, fn) => { handlers.set(name, fn); return () => handlers.delete(name); } },
    "../runes/types": runeTypes, "./champ-select-model": model, "./buildLogic": buildLogic, "./championsModel": championsModel, "./ui/format": format,
    "./types": runeTypes, "../full-app/champ-select-pro": proBuilds,
    "./runesPanelModel": runesPanelModel,
    "../settingsEvents": { useSettingsVersion: () => 0 },
    ...environment.modules,
  };
  const code = ts.transpileModule(readFileSync(new URL(file, import.meta.url), "utf8"), {
    compilerOptions: { module: ts.ModuleKind.CommonJS, jsx: ts.JsxEmit.ReactJSX, target: ts.ScriptTarget.ES2022 },
  }).outputText;
  const exports = {};
  new Function("require", "exports", "document", "window", ...Object.keys(environment.globals ?? {}), code)(name => modules[name] ?? {}, exports, document, window, ...Object.values(environment.globals ?? {}));
  const render = value => { props = value ?? props; cursor = 0; result = exports[exportName](props); pending.splice(0).forEach(fn => fn()); return result; };
  return {
    render, calls, timers, document,
    async flush() { for (let i = 0; i < 8; i++) { await new Promise(resolve => setImmediate(resolve)); render(); } return result; },
    async emit(name, payload) { handlers.get(name)?.({ payload }); return this.flush(); },
    async visible(value) { document.visibilityState = value; handlers.get("visibilitychange")?.(); return this.flush(); },
    tree: () => result,
  };
}
function nodes(tree) {
  if (!tree || typeof tree !== "object") return [];
  if (Array.isArray(tree)) return tree.flatMap(nodes);
  return [tree, ...nodes(tree.props?.children)];
}
function textOf(tree) {
  if (typeof tree === "string" || typeof tree === "number") return String(tree);
  if (!tree) return "";
  return Array.isArray(tree) ? tree.map(textOf).join("") : textOf(tree.props?.children);
}
const button = (h, text) => nodes(h.tree()).find(n => n.type === "button" && textOf(n) === text);
const build = () => ({ runes: fixtureView("locked").presets[0], spells: [4, 12], skillOrder: "QWE", items: { items: [], starters: [], options: [], coreGames: 100, coreWinPct: 51, coreLen: 0 } });
const catalog = { trees: [
  { id: 8000, name: "Precision", keystones: [{ id: 8010 }], rows: [9111, 9104, 8299].map(id => ({ runes: [{ id }] })) },
  { id: 8300, name: "Inspiration", keystones: [], rows: [8304, 8347].map(id => ({ runes: [{ id }] })) },
], shards: [] };
function panel(overrides = {}, commands = {}) {
  const view = { ...fixtureView("locked"), applied: null, spells: { ...fixtureView("locked").spells, spell1Id: 14, spell2Id: 4, applyWithRunes: false } };
  const props = { championId: 266, name: "Aatrox", lane: "top", tier: "diamond_plus", build: build(), liveView: view, ...overrides };
  const h = harness("./BuildPanel.tsx", "BuildPanel", { full_app_rune_catalog: catalog, full_app_abilities: [], get_runes: view, ...commands });
  h.render(props);
  return { h, props };
}

const itemBuild = () => ({
  starters: [{ id: 1054, name: "Doran's Shield", count: 1 }, { id: 2003, name: "Health Potion", count: 2 }],
  items: [{ id: 6692, name: "Eclipse" }, { id: 3047, name: "Plated Steelcaps" }],
  options: [{ id: 3053, name: "Sterak's Gage", slot: 4, games: 80, winPct: 52 }],
  games: 100, coreGames: 100, coreLen: 2, coreWinPct: 51, stale: false, updatedAt: null, skillPriority: "QWE",
});
const proBuild = () => ({
  ...fixtureView("locked").presets[0], matchId: "fixture-pro", proName: "Demo Pro", spells: [4, 12],
  finalItems: [...itemBuild().items, { id: 3340, name: "Trinket" }], completedItems: itemBuild().items,
  itemPath: [{ minute: 0, items: itemBuild().starters }], skillOrder: ["Q", "W", "E"],
});

test("draft Item set apply sends the chosen full build including starters", async () => {
  const chosen = itemBuild();
  const { h } = panel({ matchups: [{ enemyChampionId: 122, enemyName: "Darius", preset: build().runes, build: chosen, fallback: false }] });
  await h.flush();
  nodes(h.tree()).find(n => n.type === "button" && textOf(n).startsWith("vs Darius")).props.onClick(); h.render();
  const chip = nodes(h.tree()).find(n => n.props?.className?.includes("cs-apply-chip") && textOf(n).startsWith("Item set"));
  nodes(chip).find(n => n.type === "button").props.onClick(); await h.flush();
  assert.deepEqual(h.calls.filter(c => c.name.startsWith("import_")), [{ name: "import_full_item_build", args: { championId: 266, championName: "Aatrox", build: chosen } }]);
});

test("tray controller imports the full item build", async () => {
  const h = harness("../app/useAppController.ts", "useAppController", {}, "visible", { effects: false });
  h.render();
  const chosen = itemBuild();
  await h.tree().importItemBuild(266, "Aatrox", chosen);
  assert.deepEqual(h.calls, [{ name: "import_full_item_build", args: { championId: 266, championName: "Aatrox", build: chosen } }]);
});

for (const mode of ["desktop", "remote"]) test(`${mode} pro import preserves starter stacks and completed items`, async () => {
  const calls = [], chosen = proBuild();
  const h = harness("../runes/RunesPanel.tsx", "RunesPanel");
  h.render({ mode, view: fixtureView("locked"), sessionKey: "fixture", onLoadBuild: async () => null,
    onLoadProBuilds: async () => ({ matches: [chosen] }), onLoadMatchup: async () => null,
    onApply: async () => {}, onImportItems: async (...args) => calls.push(args), championApi: {} });
  await h.flush();
  button(h, "Pro builds").props.onClick(); await h.flush();
  const list = nodes(h.tree()).find(n => n.props?.onImport);
  list.props.onImport(chosen); await h.flush();
  assert.deepEqual(calls, [[266, "Aatrox", proBuilds.proVariant(chosen).items]]);
  assert.deepEqual(calls[0][2].starters, itemBuild().starters);
  assert.deepEqual(calls[0][2].items, chosen.completedItems);
});

test("phone item import posts the full build with its action header", async () => {
  const requests = [], view = fixtureView("locked");
  const h = harness("../remote/RemoteApp.tsx", "default", {}, "visible", {
    window: { location: { protocol: "https:", host: "fixture.invalid" } },
    globals: {
      WebSocket: class { close() {} },
      fetch: async (url, init) => {
        requests.push({ url, init });
        const body = url === "/api/status" ? { lcuConnected: true } : url === "/api/game" ? { phase: "ChampSelect", championSelect: { availableChampionIds: [] } }
          : url.startsWith("/api/runes") ? view : url === "/api/champions" ? [] : { ok: true };
        return { ok: true, json: async () => body };
      },
    },
  });
  h.render(); await h.flush();
  button(h, "Runes").props.onClick(); await h.flush();
  const runes = nodes(h.tree()).find(n => n.props?.onImportItems), chosen = itemBuild();
  await runes.props.onImportItems(266, "Aatrox", chosen);
  const request = requests.find(r => r.url === "/api/items/import");
  assert.equal(request.init.method, "POST");
  assert.equal(request.init.headers["X-Swapper-Action"], "1");
  assert.deepEqual(JSON.parse(request.init.body), { championId: 266, championName: "Aatrox", build: chosen });
});

test("Apply all explicitly writes the displayed spell pair with automatic spells off", async () => {
  const { h } = panel(); await h.flush();
  await button(h, "Apply all").props.onClick(); await h.flush();
  const writes = h.calls.filter(c => c.name === "apply_spell").map(c => c.args);
  assert.deepEqual(writes, [{ slot: "d", spellId: 12 }, { slot: "f", spellId: 4 }]);
  const shown = nodes(h.tree()).filter(n => n.props?.mode === "desktop" && [4, 12].includes(n.props?.id)).map(n => n.props.id);
  assert.deepEqual(shown, [12, 4]);
});
test("Apply all reports a spell failure", async () => {
  const { h } = panel({}, { apply_spell: () => { throw Error("Spells rejected"); } }); await h.flush();
  button(h, "Apply all").props.onClick(); await h.flush();
  assert.match(textOf(h.tree()), /Spells rejected/);
});
test("equivalent parent build objects preserve rune edits; a changed source resets them", async () => {
  const { h, props } = panel(); await h.flush();
  // Clicking a rune edits the page directly; Reset appears once it differs.
  const page = nodes(h.tree()).find(n => n.props?.catalog && n.props?.selection);
  assert.ok(page?.props.onChange, "runes are editable without an edit mode");
  page.props.onChange({ ...page.props.selection, keystone: 8010 }); h.render();
  assert.ok(button(h, "Reset"));
  const before = h.calls.filter(c => c.name === "get_keystone_build").length;
  h.render({ ...props, build: structuredClone(props.build) }); await h.flush();
  assert.ok(button(h, "Reset"), "equivalent source discarded rune edits");
  assert.equal(h.calls.filter(c => c.name === "get_keystone_build").length, before);
  h.render({ ...props, build: { ...props.build, spells: [4, 14] } }); await h.flush();
  assert.equal(button(h, "Reset"), undefined);
});
test("draft events update taken picks during planning without a local view change", async () => {
  const view = fixtureView("planning"), draft = fixtureDraft("planning");
  let buildReads = 0;
  const h = harness("./ChampionSelect.tsx", "ChampionSelect", { full_app_draft: draft });
  h.render({ view, championApi: { list: async () => [], overview: async () => null, tierList: async () => ({ rows: [] }) }, onLoadBuild: async () => { buildReads++; return null; }, onLoadMatchup: async () => null }); await h.flush();
  const before = buildReads;
  const next = { ...draft, bans: [...draft.bans, 11], enemies: [{ championId: 238, position: "top", locked: true }] };
  await h.emit("draft_changed", next);
  const suggestions = nodes(h.tree()).find(n => n.props?.taken);
  assert.equal(suggestions.props.enemyId, 238);
  assert.ok(suggestions.props.taken.has(11));
  assert.equal(buildReads, before, "enemy reveals must not reload and reset the source build");
});
test("draft events update enemy build context without reloading the source build", async () => {
  const view = fixtureView("locked"), draft = fixtureDraft("locked");
  let buildReads = 0;
  const h = harness("./ChampionSelect.tsx", "ChampionSelect", { full_app_draft: draft });
  h.render({ view, championApi: { list: async () => [], overview: async () => null, tierList: async () => ({ rows: [] }) }, onPositionChange() {}, onLoadBuild: async () => { buildReads++; return null; }, onLoadMatchup: async () => null }); await h.flush();
  const before = buildReads;
  await h.emit("draft_changed", { ...draft, enemies: [{ championId: 238, position: "top", locked: true }] });
  assert.ok(nodes(h.tree()).find(n => n.props?.enemyIds)?.props.enemyIds.includes(238));
  assert.equal(buildReads, before);
});

test("ability hover prefers resolved text and falls back for both spells and passives", async () => {
  for (const key of ["Q", "P"]) for (const tooltip of ["Deals 75/150/225 (+60% AP) damage.", null]) {
    const sheet = { championId: 103, passive: { name: "Passive", description: "Short fallback.", tooltip }, spells: [{ key: "Q", name: "Spell", description: "Short fallback.", tooltip, cooldowns: [7], costs: [], ranges: [], charges: 0, recharge: [] }] };
    const h = harness("./AbilityHover.tsx", "AbilityHover", { champion_abilities: sheet });
    h.render({ championId: 103, abilityKey: key, children: "Q" });
    h.tree().props.onOpenChange(true); await h.flush();
    const details = nodes(h.tree()).find(n => n.type?.name === "AbilityDetails");
    const text = nodes(details.type(details.props)).find(n => n.props?.className === "fa-ability-text");
    assert.equal(textOf(text), tooltip ?? "Short fallback.");
    if (tooltip) assert.ok(nodes(text).some(n => n.props?.className === "fa-ability-ap"));
  }
});
test("game-state pushes update navigation immediately and beat a stale startup read", async () => {
  let finish;
  const h = harness("./LiveGameData.ts", "useInGame", {
    live_game_active: () => new Promise(resolve => { finish = resolve; }),
  });
  h.render(); await h.flush();
  await h.emit("live_game_state", true);
  assert.equal(h.tree(), true);
  assert.equal(h.timers.size, 0, "navigation must not wait for a refresh timer");
  finish(false); await h.flush();
  assert.equal(h.tree(), true, "stale startup state overwrote the game-start event");
  await h.emit("live_game_state", false);
  assert.equal(h.tree(), false);
});

test("hidden Live Game surfaces defer initial data and ignore player events until shown", async () => {
  const h = harness("./LiveGameData.ts", "useLiveGame", { live_game_active: true, live_game_view: { gameId: 1 } }, "hidden");
  h.render(false); await h.flush();
  assert.equal(h.calls.filter(c => c.name === "live_game_view").length, 0);
  await h.emit("live_game", { gameId: 1 }); assert.equal(h.tree().view, null);
  await h.visible("visible");
  assert.equal(h.calls.filter(c => c.name === "live_game_view").length, 1);
  assert.equal(h.tree().view.gameId, 1);
});
test("inactive Live Game clears data, stops polling and discards in-flight reads", async () => {
  let finish;
  const h = harness("./LiveGameData.ts", "useLiveGame", { live_game_active: true, live_game_view: { gameId: 1 }, live_game_live: () => new Promise(resolve => { finish = resolve; }) });
  h.render(true); await h.flush(); assert.ok(h.timers.size);
  await h.emit("live_game_state", false);
  assert.equal(h.tree().view, null); assert.equal(h.timers.size, 0);
  finish?.({ gameId: 1 }); await h.flush(); assert.equal(h.tree().view, null);
});
test("a visible active game keeps retrying when its initial view is still loading", async () => {
  const h = harness("./LiveGameData.ts", "useLiveGame", { live_game_active: true,
    live_game_view: () => { throw Error("Still loading"); }, live_game_live: { gameId: 1 } });
  h.render(true); await h.flush();
  assert.equal(h.timers.size, 1);
  assert.equal(h.tree().view.gameId, 1);
});

test("a shard that appears in two rows is chosen only in its own row", async () => {
  const { shardChosen } = await import("../runes/types.ts");
  const hecarim = { shards: [5008, 5001, 5011] }; // adaptive offense, health flex, health scaling defense
  assert.equal(shardChosen(hecarim, 0, 5008), true);
  assert.equal(shardChosen(hecarim, 1, 5008), false);
  assert.equal(shardChosen(hecarim, 1, 5001), true);
  assert.equal(shardChosen(hecarim, 2, 5001), false);
});
