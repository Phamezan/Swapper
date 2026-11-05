import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import ts from "typescript";
import { ahriVsZed } from "./fixtures/ahriVsZed.ts";

function render(state) {
  const calls = [];
  const jsx = (type, props) => typeof type === "function" ? type(props) : ({ type, props });
  const modules = {
    "react/jsx-runtime": { jsx, jsxs: jsx },
    "./useMatchupGuide": { useMatchupGuide: (...args) => { calls.push(args); return state; } },
    "./MatchupDataCardView": { DataCardLabel: () => jsx("span", { children: "Data-based" }), DataCardOverview: "data-overview" },
    "./MatchupGuideSummary": { MatchupGuideSummary: "shared-summary" },
  };
  const code = ts.transpileModule(readFileSync(new URL("./ChampionSelectMatchup.tsx", import.meta.url), "utf8"), {
    compilerOptions: { module: ts.ModuleKind.CommonJS, jsx: ts.JsxEmit.ReactJSX, target: ts.ScriptTarget.ES2022 },
  }).outputText;
  const exports = {};
  new Function("require", "exports", code)(key => modules[key] ?? {}, exports);
  const opened = [];
  const tree = exports.ChampionSelectMatchup({ champion: { id: 103, name: "Ahri" }, enemyId: 238, enemyName: "Zed", lane: "mid", onOpenMatchupGuide: (...args) => opened.push(args) });
  return { tree, calls, opened };
}
function nodes(tree) {
  if (!tree || typeof tree !== "object") return [];
  return Array.isArray(tree) ? tree.flatMap(nodes) : [tree, ...nodes(tree.props?.children)];
}
function text(tree) {
  if (typeof tree === "string" || typeof tree === "number") return String(tree);
  if (!tree || typeof tree !== "object") return "";
  return Array.isArray(tree) ? tree.map(text).join("") : text(tree.props?.children);
}

test("missing guides show only a nonempty record, otherwise nothing", () => {
  for (const record of [null, undefined, { wins: 0, losses: 0 }]) assert.equal(render({ guide: null, record }).tree, null);
  const { tree } = render({ guide: null, record: { wins: 5, losses: 20 } });
  assert.match(text(tree), /5W – 20L/);
  assert.equal(nodes(tree).some(n => n.type === "details" || n.type === "button" || n.type === "shared-summary"), false);
});

test("the compact guide reuses the shared summary and opens the exact pair and lane", () => {
  const abilities = { 103: [{ key: "E", name: "Charm", icon: null }] };
  const h = render({ guide: ahriVsZed, record: { wins: 5, losses: 20 }, abilities });
  assert.deepEqual(h.calls, [[103, 238, "mid", "emerald_plus", true]]);
  assert.equal(h.tree.type, "details");
  assert.ok(!h.tree.props.open, "starts collapsed to keep builds in view");
  assert.match(text(h.tree), /Skill matchup/);
  assert.match(text(h.tree), /5W – 20L/);
  const summary = nodes(h.tree).find(n => n.type === "shared-summary");
  assert.equal(summary.props.guide, ahriVsZed);
  assert.equal(summary.props.abilities, abilities);
  nodes(h.tree).find(n => n.type === "button").props.onClick();
  assert.deepEqual(h.opened, [[{ id: 103, name: "Ahri" }, 238, "mid"]]);
  assert.doesNotMatch(text(render({ guide: ahriVsZed, record: { wins: 0, losses: 0 } }).tree), /Your record|0W|0L/);
});

test("general guides are labelled and omit the matchup outlook", () => {
  const { enemy, difficulty, favoredWhen, enemyTips, ...general } = ahriVsZed;
  const h = render({ guide: general, record: null, abilities: {} });
  assert.match(text(h.tree), /General Ahri guide \u00b7 no Zed matchup guide yet/);
  assert.equal(nodes(h.tree).some(n => n.props.className === "cs-guide-difficulty" || n.props.className === "cs-guide-outlook"), false);
  assert.equal(nodes(h.tree).find(n => n.type === "shared-summary").props.guide, general);
});

test("data summary has the visible label, separate record and full-guide action without authored claims", () => {
  const content = { kind: "data-based", championId: 103, enemyChampionId: 238, lane: "mid", tier: "emerald_plus" };
  const h = render({ guide: null, content, record: { wins: 5, losses: 20 } });
  assert.match(text(h.tree), /Data-based/);
  assert.match(text(h.tree), /5W – 20L/);
  assert.doesNotMatch(text(h.tree), /Reviewed|Not reviewed|Skill matchup|favou?red|difficulty/);
  assert.equal(nodes(h.tree).find(n => n.type === "data-overview").props.card, content);
  assert.equal(nodes(h.tree).some(n => n.type === "shared-summary"), false);
  nodes(h.tree).find(n => n.type === "button").props.onClick();
  assert.deepEqual(h.opened, [[{ id: 103, name: "Ahri" }, 238, "mid"]]);
});
