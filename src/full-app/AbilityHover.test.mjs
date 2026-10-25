import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import ts from "typescript";
import * as cooldownModel from "./cooldownModel.ts";

// Run the real components with deterministic hooks, command responses and timers.
function harness(exportName, command, file = "./AbilityHover.tsx") {
  const states = [], effects = [], timers = new Map(), calls = [];
  let cursor = 0, pending = [], tree, props, nextTimer = 0;
  const hooks = {
    useState(initial) {
      const i = cursor++;
      if (!(i in states)) states[i] = typeof initial === "function" ? initial() : initial;
      return [states[i], value => { states[i] = typeof value === "function" ? value(states[i]) : value; }];
    },
    useEffect(fn, deps) {
      const i = cursor++;
      if (!effects[i] || deps.some((value, j) => !Object.is(value, effects[i].deps[j]))) {
        pending.push(() => { effects[i]?.cleanup?.(); effects[i] = { deps, cleanup: fn() }; });
      }
    },
  };
  const setTimeout = fn => { const id = ++nextTimer; timers.set(id, fn); return id; };
  const clearTimeout = id => timers.delete(id);
  const jsx = (type, props) => ({ type, props });
  const modules = {
    react: hooks, "react/jsx-runtime": { jsx, jsxs: jsx, Fragment: "fragment" },
    "@tauri-apps/api/core": { isTauri: () => true, invoke: async (name, args) => { calls.push({ name, args }); return command(args); } },
    "./cooldownModel": cooldownModel,
    "./CooldownSheet": { AbilityIcon: "ability-icon" },
    "radix-ui": { HoverCard: { Root: "hover-root", Trigger: "hover-trigger", Portal: "hover-portal", Content: "hover-content" } },
  };
  const code = ts.transpileModule(readFileSync(new URL(file, import.meta.url), "utf8"), {
    compilerOptions: { module: ts.ModuleKind.CommonJS, jsx: ts.JsxEmit.ReactJSX, target: ts.ScriptTarget.ES2022 },
  }).outputText;
  const exports = {};
  new Function("require", "exports", "setTimeout", "clearTimeout", "window", code)(
    name => modules[name] ?? {}, exports, setTimeout, clearTimeout, { setTimeout, clearTimeout },
  );
  const render = value => { props = value ?? props; cursor = 0; tree = exports[exportName](props); pending.splice(0).forEach(fn => fn()); return tree; };
  return {
    render, calls, timers, tree: () => tree,
    async flush() { for (let i = 0; i < 5; i++) { await new Promise(setImmediate); render(); } },
    async open(value = true) { tree.props.onOpenChange(value); render(); await this.flush(); },
    async tick() { const callbacks = [...timers.values()]; timers.clear(); callbacks.forEach(fn => fn()); await this.flush(); },
    unmount() { effects.forEach(effect => effect?.cleanup?.()); },
  };
}
const sheet = (championId, tooltipsReady, tooltip = null) => ({
  championId, tooltipsReady, summoners: [],
  passive: { name: "Passive", iconPath: null, description: "Passive fallback.", tooltip: null },
  spells: [{ key: "Q", name: `Spell ${championId}`, iconPath: null, description: "Spell fallback.", tooltip, cooldowns: [7], costs: [], ranges: [], charges: 0, recharge: [] }],
});
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
function details(h) {
  return nodes(h.tree()).filter(node => node.type?.name === "AbilityDetails").map(node => textOf(node.type(node.props))).join(" ");
}

for (const component of ["AbilityHover", "ChampionAbilitiesHover"]) {
  const props = { championId: 103, abilityKey: "Q", children: "trigger" };

  test(`${component} shows base data and refreshes missing tooltips while open`, async () => {
    let ready = false;
    const h = harness(component, () => sheet(103, ready, ready ? "Deals 10/20/30 damage." : null));
    h.render(props); await h.open();
    assert.match(details(h), /Spell fallback/);
    assert.equal(h.timers.size, 1, "pending tooltips need a refresh");
    ready = true; await h.tick();
    assert.match(details(h), /Deals 10\/20\/30 damage/);
    assert.equal(h.calls.length, 2, "the pending promise must not pin the base sheet");
    assert.equal(h.timers.size, 0);
    await h.open(false); await h.open();
    assert.equal(h.calls.length, 2, "completed sheets stay cached");
    h.unmount();
  });

  test(`${component} cancels refresh on close and retries when reopened`, async () => {
    const h = harness(component, () => sheet(103, false));
    h.render(props); await h.open(); await h.open(false);
    assert.equal(h.timers.size, 0);
    await h.open();
    assert.equal(h.calls.length, 2);
    h.unmount(); assert.equal(h.timers.size, 0);
  });

  test(`${component} caches a successful lookup that genuinely has no tooltip text`, async () => {
    const h = harness(component, () => sheet(103, true));
    h.render(props); await h.open();
    assert.match(details(h), /Spell fallback/);
    assert.equal(h.timers.size, 0);
    await h.open(false); await h.open();
    assert.equal(h.calls.length, 1);
    h.unmount();
  });

  test(`${component} keeps the League note on failure and retries on reopen`, async () => {
    let failed = true;
    const h = harness(component, () => { if (failed) throw Error("offline"); return sheet(103, true); });
    h.render(props); await h.open();
    assert.match(textOf(h.tree()), /need League open/);
    failed = false; await h.open(false); await h.open();
    assert.match(details(h), /Spell fallback/);
    assert.equal(h.calls.length, 2);
    h.unmount();
  });

  test(`${component} ignores stale results after the champion changes`, async () => {
    let finish;
    const h = harness(component, ({ championId }) => championId === 103 ? new Promise(resolve => { finish = resolve; }) : sheet(championId, true));
    h.render(props); await h.open();
    h.render({ ...props, championId: 202 }); await h.flush();
    finish(sheet(103, false)); await h.flush();
    assert.match(textOf(h.tree()), /Spell 202/);
    assert.doesNotMatch(textOf(h.tree()), /Spell 103/);
    assert.equal(h.timers.size, 0);
    h.unmount();
  });
}

test("cooldown sheets render base cooldowns with tooltips still pending", async () => {
  const h = harness("CooldownSheet", ({ championId }) => sheet(championId, false), "./CooldownSheet.tsx");
  h.render({ championId: 103, spells: [4, 12] }); await h.flush();
  assert.match(textOf(h.tree()), /Spell 103/);
  assert.match(textOf(h.tree()), /7 s/);
  assert.deepEqual(h.calls[0].args, { championId: 103, spellIds: [4, 12] });
  h.unmount();
});
