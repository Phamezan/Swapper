import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import ts from "typescript";
import * as runeTypes from "../runes/types.ts";
import * as model from "./champ-select-model.ts";
import * as buildLogic from "./buildLogic.ts";
import * as format from "./ui/format.ts";
import { fixtureView, fixtureDraft } from "./champ-select-fixtures.ts";

// Execute the actual components with deterministic hooks and command/event boundaries.
function harness(file, exportName, commands = {}, visibility = "visible") {
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
  };
  const invoke = async (name, args) => {
    calls.push({ name, args });
    return typeof commands[name] === "function" ? commands[name](args) : commands[name] ?? null;
  };
  const jsx = (type, props) => ({ type, props });
  const modules = {
    react: hooks, "react/jsx-runtime": { jsx, jsxs: jsx, Fragment: "fragment" },
    "@tauri-apps/api/core": { invoke, isTauri: () => true },
    "@tauri-apps/api/event": { listen: async (name, fn) => { handlers.set(name, fn); return () => handlers.delete(name); } },
    "../runes/types": runeTypes, "./champ-select-model": model, "./buildLogic": buildLogic, "./ui/format": format,
  };
  const code = ts.transpileModule(readFileSync(new URL(file, import.meta.url), "utf8"), {
    compilerOptions: { module: ts.ModuleKind.CommonJS, jsx: ts.JsxEmit.ReactJSX, target: ts.ScriptTarget.ES2022 },
  }).outputText;
  const exports = {};
  new Function("require", "exports", "document", "window", code)(name => modules[name] ?? {}, exports, document, window);
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
  const h = harness("./BuildPanel.tsx", "BuildPanel", { full_app_rune_catalog: catalog, get_runes: view, ...commands });
  h.render(props);
  return { h, props };
}

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
test("draft events update taken picks and enemy build context without a local view change", async () => {
  const view = fixtureView("hover"), draft = fixtureDraft("hover");
  let buildReads = 0;
  const h = harness("./ChampionSelect.tsx", "ChampionSelect", { full_app_draft: draft });
  h.render({ view, championApi: { list: async () => [], overview: async () => null }, onLoadBuild: async () => { buildReads++; return null; }, onLoadMatchup: async () => null }); await h.flush();
  const before = buildReads;
  const next = { ...draft, bans: [...draft.bans, 11], enemies: [{ championId: 238, position: "top", locked: true }] };
  await h.emit("draft_changed", next);
  const suggestions = nodes(h.tree()).find(n => n.props?.taken);
  assert.equal(suggestions.props.enemyId, 238);
  assert.ok(suggestions.props.taken.has(11));
  assert.equal(buildReads, before, "enemy reveals must not reload and reset the source build");
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
