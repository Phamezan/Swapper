import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import ts from "typescript";
import * as visuals from "./matchupVisuals.ts";
import * as guideModel from "./matchupGuide.ts";
import { perRank } from "./cooldownModel.ts";

function harness(file, component, command, sheets = {}, storage = { getItem: () => null, setItem() {} }) {
  const states = [], effects = [], calls = [];
  let cursor = 0, pending = [], props, tree;
  const react = {
    useId: () => "visual-content",
    useRef: () => ({ current: null }),
    useState(initial) { const i = cursor++; if (!(i in states)) states[i] = typeof initial === "function" ? initial() : initial; return [states[i], value => { states[i] = value; }]; },
    useEffect(fn, deps) {
      const i = cursor++;
      if (!effects[i] || deps.some((value, j) => !Object.is(value, effects[i].deps[j]))) pending.push(() => { effects[i]?.cleanup?.(); effects[i] = { deps, cleanup: fn() }; });
    },
  };
  const jsx = (type, props) => ({ type, props });
  const modules = {
    react, "react/jsx-runtime": { jsx, jsxs: jsx, Fragment: "fragment" },
    "@tauri-apps/api/core": { invoke: async (name, args) => { calls.push({ name, args }); return command(name, args); } },
    "./matchupGuide": guideModel,
    "./matchupVisuals": visuals, "./cooldownModel": { perRank },
    "./GuideItem": { GuideItem: "item" }, "./CooldownSheet": { AbilityIcon: "ability-icon" },
    "./GuideVisual": { GuideVisual: "visual" }, "lucide-react": { ChevronDown: "chevron" },
    "./AbilityHover": { useAbilitySheet: id => ({ sheet: sheets[id] ?? null }) },
  };
  const code = ts.transpileModule(readFileSync(new URL(file, import.meta.url), "utf8"), { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022, jsx: ts.JsxEmit.ReactJSX } }).outputText;
  const exports = {};
  new Function("require", "exports", "localStorage", code)(name => modules[name] ?? {}, exports, storage);
  const render = value => { props = value ?? props; cursor = 0; tree = exports[component](props); pending.splice(0).forEach(fn => fn()); return tree; };
  return { render, calls, async flush() { for (let i = 0; i < 5; i++) await new Promise(setImmediate); return render(); } };
}
function nodes(tree) { return !tree || typeof tree !== "object" ? [] : Array.isArray(tree) ? tree.flatMap(nodes) : [tree, ...nodes(tree.props?.children)]; }
const props = { champion: { id: 103, name: "Ahri" }, enemy: { id: 238, name: "Zed" }, lane: "mid", tier: "emerald_plus", matchup: { enemyChampionId: 238, build: { items: [{ id: 3118, name: "Malignance" }] }, fallback: false } };

test("timeline requests the enemy's usual build at the same lane/tier and retains your matchup path on failure", async () => {
  const h = harness("./PowerSpikeTimeline.tsx", "PowerSpikeTimeline", () => { throw new Error("Unavailable"); });
  h.render(props);
  const tree = await h.flush();
  assert.deepEqual(h.calls, [{ name: "get_champion_overview", args: { championId: 238, position: "mid", tier: "emerald_plus" } }]);
  assert.equal(nodes(tree).filter(node => node.props.className === "mg-spike-side is-you").length, 1);
  assert.equal(nodes(tree).filter(node => node.props.className === "mg-spike-side is-enemy").length, 0);
  assert.equal(nodes(tree).find(node => node.type === "item").props.id, 3118);
});

test("timeline hides with no build data and shows an enemy-only path when only that request succeeds", async () => {
  const empty = harness("./PowerSpikeTimeline.tsx", "PowerSpikeTimeline", () => ({ build: null }));
  assert.equal(empty.render({ ...props, matchup: null }), null);
  assert.equal(await empty.flush(), null);
  const enemy = harness("./PowerSpikeTimeline.tsx", "PowerSpikeTimeline", () => ({ build: { items: { items: [{ id: 6692, name: "Eclipse" }] } } }));
  enemy.render({ ...props, matchup: null });
  const tree = await enemy.flush();
  assert.equal(nodes(tree).filter(node => node.props.className === "mg-spike-side is-enemy").length, 1);
  assert.equal(nodes(tree).find(node => node.type === "item").props.id, 6692);
});

test("trade chart shows every rank without a picker and keeps ultimates on a separate scale", () => {
  const ability = key => ({ key, name: key, iconPath: null, cooldowns: key === "R" ? [120, 100, 80] : [20, 18, 16, 14, 12], charges: 0, recharge: [] });
  const sheet = { spells: ["Q", "W", "E", "R"].map(ability) };
  const missing = harness("./TradeWindows.tsx", "TradeWindows", () => null, { 103: sheet });
  assert.equal(missing.render(props), null);
  const ready = harness("./TradeWindows.tsx", "TradeWindows", () => null, { 103: sheet, 238: sheet });
  const tree = ready.render(props);
  assert.equal(nodes(tree).some(node => node.type === "picker"), false);
  const basic = nodes(tree).find(node => node.props.className === "mg-trade-basics");
  const ultimate = nodes(tree).find(node => node.props.className === "mg-trade-ultimates");
  assert.equal(nodes(basic).filter(node => node.props.className?.startsWith("mg-trade-row ")).length, 6);
  assert.equal(nodes(ultimate).filter(node => node.props.className?.startsWith("mg-trade-row ")).length, 2);
  assert.match(nodes(basic).find(node => node.props.className === "mg-trade-tick").props["aria-label"], /Rank 2: 18s/);
  assert.equal(nodes(basic).find(node => node.props.className === "mg-trade-row is-you").props.style["--bar"], "100%");
});

test("both visual preferences default expanded, persist independently, and survive denied storage", () => {
  const values = new Map();
  const storage = { getItem: key => values.get(key) ?? null, setItem: (key, value) => values.set(key, value) };
  for (const preference of ["trades", "spikes"]) {
    const props = { title: "Test visual", preference, children: "Chart" };
    const create = access => harness("./GuideVisual.tsx", "GuideVisual", () => null, {}, access);
    const h = create(storage);
    const toggle = () => nodes(h.render(props)).find(node => node.type === "button");
    assert.equal(toggle().props["aria-expanded"], true);
    toggle().props.onClick();
    assert.equal(toggle().props["aria-expanded"], false);
    assert.equal(nodes(h.render()).find(node => node.props.id === "visual-content").props.hidden, true);
    const restored = create(storage);
    assert.equal(nodes(restored.render(props)).find(node => node.type === "button").props["aria-expanded"], false);
    const denied = create({ getItem() { throw new Error("denied"); }, setItem() { throw new Error("denied"); } });
    const initial = nodes(denied.render(props)).find(node => node.type === "button");
    assert.equal(initial.props["aria-expanded"], true);
    assert.doesNotThrow(() => initial.props.onClick());
    assert.equal(nodes(denied.render()).find(node => node.type === "button").props["aria-expanded"], false);
  }
  assert.equal(values.size, 2);
});


test("guide card collapse hides all content, persists, and survives unavailable storage", () => {
  const values = new Map();
  const storage = { getItem: key => values.get(key) ?? null, setItem: (key, value) => values.set(key, value) };
  const create = access => harness("./MatchupGuideCarousel.tsx", "MatchupGuideCarousel", () => null, {}, access);
  const input = { ...props, guide: null, loading: false, abilities: {} };
  const previousWindow = globalThis.window;
  globalThis.window = { addEventListener() {}, removeEventListener() {} };
  try {
    const h = create(storage);
    const toggle = () => nodes(h.render(input)).find(n => n.type === "button" && n.props["aria-expanded"] !== undefined);
    assert.ok(toggle(), "guide header has a collapse button");
    assert.equal(toggle().props["aria-expanded"], true);
    assert.equal(toggle().props.type, "button", "native button supports Enter and Space");
    const id = toggle().props["aria-controls"];
    assert.ok(id);
    toggle().props.onClick();
    assert.equal(toggle().props["aria-expanded"], false);
    assert.equal(nodes(h.render()).find(n => n.props.id === id).props.hidden, true);
    assert.equal(nodes(create(storage).render(input)).find(n => n.props["aria-controls"] === id).props["aria-expanded"], false);
    toggle().props.onClick();
    assert.equal(toggle().props["aria-expanded"], true);
    const denied = create({ getItem() { throw Error("denied"); }, setItem() { throw Error("denied"); } });
    const button = nodes(denied.render(input)).find(n => n.props["aria-controls"] === id);
    assert.equal(button.props["aria-expanded"], true);
    assert.doesNotThrow(() => button.props.onClick());
    assert.equal(nodes(denied.render()).find(n => n.props.id === id).props.hidden, true);
  } finally { globalThis.window = previousWindow; }
});
