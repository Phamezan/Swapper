import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import ts from "typescript";
import * as model from "./matchupDataCard.ts";
import * as guideModel from "./matchupGuide.ts";

function load(file, modules, globals = {}) {
  const code = ts.transpileModule(readFileSync(new URL(file, import.meta.url), "utf8"), {
    compilerOptions: { module: ts.ModuleKind.CommonJS, jsx: ts.JsxEmit.ReactJSX, target: ts.ScriptTarget.ES2022 },
  }).outputText;
  const exports = {};
  new Function("require", "exports", ...Object.keys(globals), code)(name => modules[name] ?? {}, exports, ...Object.values(globals));
  return exports;
}
const jsx = (type, props) => typeof type === "function" ? type(props) : { type, props };
const runtime = { jsx, jsxs: jsx, Fragment: "fragment" };
const nodes = tree => !tree || typeof tree !== "object" ? [] : Array.isArray(tree) ? tree.flatMap(nodes) : [tree, ...nodes(tree.props?.children)];
const text = tree => typeof tree === "string" || typeof tree === "number" ? String(tree) : !tree ? "" : Array.isArray(tree) ? tree.map(text).join("") : text(tree.props?.children);
const makeCard = () => ({ kind: "data-based", championId: 103, enemyChampionId: 238, lane: "mid", tier: "emerald_plus",
  stats: { wins: 460, games: 1000, patch: "16.22", dataVersion: "1.5.0", fetchedAt: Date.now(), stale: true },
  enemyKit: { version: "16.22", fetchedAt: Date.now(), sheet: { championId: 238, spells: ["Q","W"].map(key => ({ key, name: `${key} spell`, iconPath: null, cooldowns: [20,18,16,14,12], ranges: [], charges: 0, recharge: [] })) } } });
const props = { champion: { id: 103, name: "Ahri" }, enemy: { id: 238, name: "Zed" }, lane: "mid", tier: "emerald_plus", matchup: null, guide: null, loading: false, abilities: {} };

test("full data overview renders the label, cache provenance and sampling detail as plain text", () => {
  const view = load("./MatchupDataCardView.tsx", {
    "react/jsx-runtime": runtime, "./matchupDataCard": model, "../runes/types": { TIER_OPTIONS: [{ value: "emerald_plus", label: "Emerald+" }] },
    "./AbilityHover": { AbilityHover: "pinned-hover" }, "./CooldownSheet": { AbilityIcon: "icon" },
  });
  const card = makeCard(), tree = view.DataCardOverview({ ...props, card });
  assert.match(text(tree), /Data-based/);
  assert.match(text(tree), /Game win rate/);
  assert.match(text(tree), /46.0% of 1,000 games/);
  assert.match(text(tree), /95% interval/);
  assert.match(text(tree), /Cached · fetched/);
  assert.match(text(tree), /Enemy kit version 16.22/);
  assert.doesNotMatch(text(tree), /Reviewed|Not reviewed|Who has the edge|Three things|favou?red|Win condition/);
  const abilities = view.DataCardEnemyAbilities({ card, enemy: props.enemy });
  assert.ok(nodes(abilities).filter(n => n.type === "pinned-hover").every(n => n.props.sheet === card.enemyKit.sheet));
  assert.match(text(abilities), /Base values exclude ability haste/);
  const literal = view.DataCardOverview({ ...props, card, champion: { id: 103, name: "<script>example</script>" } });
  assert.match(text(literal), /<script>example<\/script>/);
  assert.ok(nodes(literal).every(n => !n.props.dangerouslySetInnerHTML));
});

function carousel(initial) {
  const state = [], effects = [], listeners = new Map(); let cursor = 0, pending = [], props = initial;
  const hooks = {
    useId: () => "deck", useRef(initial) { const i = cursor++; return state[i] ??= { current: initial }; },
    useState(initial) { const i = cursor++; if (!(i in state)) state[i] = typeof initial === "function" ? initial() : initial; return [state[i], value => { state[i] = value; }]; },
    useEffect(fn, deps) { const i = cursor++; if (!effects[i] || deps.some((v,j) => v !== effects[i].deps[j])) pending.push(() => { effects[i]?.cleanup?.(); effects[i] = { deps, cleanup: fn() }; }); },
  };
  const component = load("./MatchupGuideCarousel.tsx", {
    react: hooks, "react/jsx-runtime": runtime, "./matchupDataCard": model, "./matchupGuide": guideModel,
    "./MatchupDataCardView": { DataCardOverview: "data-overview", DataCardEnemyAbilities: "data-abilities" },
    "./TradeWindows": { TradeWindows: "trade-windows" }, "./PowerSpikeTimeline": { PowerSpikeTimeline: "power-timeline" },
    "./MatchupGuideSummary": { MatchupGuideSummary: "authored-summary" }, "./MatchupGuideContent": { GuideSectionContent: "authored-section" },
  }, { window: { addEventListener: (name, fn) => listeners.set(name, fn), removeEventListener: name => listeners.delete(name) }, HTMLElement: class {} });
  const render = next => { props = next ?? props; cursor = 0; const tree = component.MatchupGuideCarousel(props); pending.splice(0).forEach(fn => fn()); return tree; };
  return { render, key(key) { listeners.get("keydown")({ key, preventDefault() {}, target: null }); return render(); } };
}

test("data carousel uses only available sections and keyboard navigation uses that count", () => {
  const content = makeCard(), h = carousel({ ...props, content });
  const tabs = tree => nodes(tree).filter(n => n.props.role === "tab");
  let tree = h.render();
  assert.deepEqual(tabs(tree).map(text), ["Matchup data", "Enemy abilities"]);
  assert.equal(nodes(tree).some(n => n.type === "trade-windows" || n.type === "power-timeline" || n.type === "authored-summary"), false);
  assert.equal(tabs(h.key("End"))[1].props["aria-selected"], true);
  assert.equal(tabs(h.key("ArrowRight"))[0].props["aria-selected"], true);
  assert.equal(tabs(h.key("ArrowLeft"))[1].props["aria-selected"], true);
  const goldOnly = { ...content, enemyKit: undefined };
  h.render({ ...props, content: goldOnly }); tree = h.render();
  assert.equal(tabs(tree).length, 1);
  assert.equal(tabs(tree)[0].props["aria-selected"], true);
  assert.equal(nodes(tree).some(n => /^(Previous|Next) guide section$/.test(n.props["aria-label"])), false);
  assert.equal(tabs(h.key("End"))[0].props["aria-selected"], true);
});

test("authored carousel retains its four sections and resets to overview when source changes", () => {
  const guide = { champion: 103, enemy: 238, lane: "mid" }, h = carousel({ ...props, guide, content: { kind: "authored", guide }, initialSection: "early-lane" });
  let tree = h.render();
  assert.equal(nodes(tree).filter(n => n.props.role === "tab").length, 4);
  assert.equal(nodes(tree).filter(n => n.props.role === "tab")[1].props["aria-selected"], true);
  assert.equal(nodes(tree).some(n => n.type === "trade-windows"), true);
  h.render({ ...props, content: makeCard() }); tree = h.render();
  assert.equal(nodes(tree).filter(n => n.props.role === "tab")[0].props["aria-selected"], true);
});
