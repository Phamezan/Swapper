import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import ts from "typescript";
import * as logic from "./matchupVodModel.ts";
import * as guide from "./matchupGuide.ts";
import { ahriVsZedVods } from "./fixtures/ahriVsZedVods.ts";

function harness(file, component, vods = ahriVsZedVods) {
  const states = [], effects = [], focus = [], domRefs = new Set();
  let cursor = 0, pending = [], props;
  const react = {
    useId: () => "test-id", useRef(initial) { const i = cursor++; return states[i] ??= { current: initial }; },
    useState(initial) { const i = cursor++; if (!(i in states)) states[i] = typeof initial === "function" ? initial() : initial; return [states[i], value => { states[i] = value; }]; },
    useEffect(fn, deps) { const i = cursor++; if (!effects[i] || deps.some((dep, j) => dep !== effects[i].deps[j])) pending.push(() => { effects[i]?.cleanup?.(); effects[i] = { deps, cleanup: fn() }; }); },
  };
  const jsx = (type, props) => ({ type, props });
  const modules = {
    react, "react/jsx-runtime": { jsx, jsxs: jsx, Fragment: "fragment" }, "lucide-react": { Play: "play", ChevronRight: "chevron", ArrowLeft: "back-icon" },
    "./matchupVodModel": logic, "./matchupGuide": guide, "./loadMatchupVods": { loadMatchupVods: async () => vods },
    "./art": { useChampionArt: () => null }, "./GuideSourceButton": { GuideSourceButton: "source-button" },
    "./VodPlayer": { VodPlayer: "player-identity" }, "../runes/TeamLogo": { TeamLogo: "team-logo" }, "../runes/RankCrest": { RankCrest: "rank-crest" },
    "./ui/States": { Skeleton: "skeleton" }, "./MatchupGuideContent": { GuideSectionContent: "guide-content" },
    "./MatchupGuideSummary": { MatchupGuideSummary: "summary" }, "./TradeWindows": { TradeWindows: "trades" }, "./PowerSpikeTimeline": { PowerSpikeTimeline: "spikes" },
  };
  const source = readFileSync(new URL(file, import.meta.url), "utf8");
  const code = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS, jsx: ts.JsxEmit.ReactJSX, target: ts.ScriptTarget.ES2022 } }).outputText;
  const exports = {};
  new Function("require", "exports", "window", code)(name => modules[name] ?? {}, exports, { addEventListener() {}, removeEventListener() {} });
  const render = value => {
    props = value ?? props; cursor = 0; const tree = exports[component](props);
    for (const ref of domRefs) ref.current = null;
    for (const node of nodes(tree)) if (node.props.ref && typeof node.props.ref === "object") {
      const ref = node.props.ref; domRefs.add(ref); ref.current = { querySelector: () => null, focus: () => focus.push(node.props["data-vod-id"] ?? node.props.className) };
    }
    pending.splice(0).forEach(fn => fn()); return tree;
  };
  return { render, focus, async flush() { for (let i = 0; i < 5; i++) await new Promise(setImmediate); return render(); } };
}
function nodes(tree) { return !tree || typeof tree !== "object" ? [] : Array.isArray(tree) ? tree.flatMap(nodes) : [tree, ...nodes(tree.props?.children)]; }
function text(tree) { return typeof tree === "string" || typeof tree === "number" ? String(tree) : !tree || typeof tree !== "object" ? "" : Array.isArray(tree) ? tree.map(text).join("") : text(tree.props?.children); }
const props = { champion: { id: 103, name: "Ahri" }, enemy: { id: 238, name: "Zed" }, lane: "mid", onGuideSection() {} };

test("VODs start as a five-game list and Back restores focus to the opened row", async () => {
  const h = harness("./MatchupVods.tsx", "MatchupVods");
  h.render(props);
  let tree = await h.flush();
  assert.equal(nodes(tree).filter(node => node.props.className === "mv-vod-row").length, 5);
  assert.equal(nodes(tree).some(node => node.props.className === "mv-frame"), false);
  assert.doesNotMatch(text(tree), /Played as/);
  nodes(tree).filter(node => node.props.className === "mv-vod-row")[2].props.onClick();
  tree = h.render();
  assert.equal(text(nodes(tree).find(node => node.props.className === "mv-title")), ahriVsZedVods[2].title);
  assert.equal(h.focus[h.focus.length - 1], "mv-back");
  nodes(tree).find(node => node.props.className === "mv-back").props.onClick();
  tree = h.render();
  assert.equal(nodes(tree).filter(node => node.props.className === "mv-vod-row").length, 5);
  assert.equal(h.focus[h.focus.length - 1], ahriVsZedVods[2].videoId);
});

test("Escape leaves the media view without triggering the parent matchup navigation", async () => {
  const h = harness("./MatchupVods.tsx", "MatchupVods");
  h.render(props);
  const list = await h.flush();
  nodes(list).find(node => node.props.className === "mv-vod-row").props.onClick();
  const player = h.render();
  let prevented = false, stopped = false;
  player.props.onKeyDown({ key: "Escape", preventDefault() { prevented = true; }, stopPropagation() { stopped = true; } });
  assert.equal(prevented && stopped, true);
  assert.equal(nodes(h.render()).filter(node => node.props.className === "mv-vod-row").length, 5);
  assert.equal(h.focus[h.focus.length - 1], ahriVsZedVods[0].videoId);
});

test("fixture playback stays inert while moment tags navigate to their exact guide section", async () => {
  const sections = [];
  const h = harness("./MatchupVods.tsx", "MatchupVods");
  h.render({ ...props, onGuideSection: section => sections.push(section) });
  const list = await h.flush();
  nodes(list).find(node => node.props.className === "mv-vod-row").props.onClick();
  const tree = h.render();
  for (const opener of nodes(tree).filter(node => typeof node.type === "function" && node.type.name === "VodOpen")) {
    const button = opener.type(opener.props);
    assert.equal(button.type, "button");
    assert.equal(button.props["aria-disabled"], true);
    assert.equal(button.props.onClick, undefined);
  }
  nodes(tree).find(node => node.props.className === "mv-section-link").props.onClick();
  assert.deepEqual(sections, ["early-lane"]);
  assert.match(text(tree), /Preview data/);
});

test("player identity uses desktop team logos for pros and a rank crest for solo players", () => {
  const h = harness("./VodPlayer.tsx", "VodPlayer");
  let tree = h.render({ vod: { ...ahriVsZedVods[0], team: "T1" } });
  const team = nodes(tree).find(node => node.type === "team-logo");
  assert.equal(team.props.team, "T1");
  assert.equal(team.props.mode, "desktop");
  assert.equal(nodes(tree).some(node => node.type === "rank-crest"), false);
  tree = h.render({ vod: { ...ahriVsZedVods[0], team: undefined, rankClaim: "Grandmaster" } });
  const crest = nodes(tree).find(node => node.type === "rank-crest");
  assert.equal(crest.props.tier, "grandmaster");
  assert.equal(crest.props.mode, "desktop");
});

test("missing and failed player photos show initials rather than a broken image", () => {
  const h = harness("./VodPlayer.tsx", "VodPlayer");
  const vod = { ...ahriVsZedVods[0], player: "Fixture Pro 1" };
  const avatar = tree => nodes(tree).find(node => node.props.className === "mv-avatar");
  assert.equal(text(avatar(h.render({ vod }))), "FP");
  const withPhoto = { ...vod, playerImage: "https://example.invalid/player.png" };
  const image = nodes(h.render({ vod: withPhoto })).find(node => node.type === "img");
  image.props.onError();
  const failed = h.render();
  assert.equal(nodes(failed).some(node => node.type === "img"), false);
  assert.equal(text(avatar(failed)), "FP");
});

test("an empty VOD response shows a compact empty message", async () => {
  const h = harness("./MatchupVods.tsx", "MatchupVods", []);
  h.render(props);
  assert.equal(text(await h.flush()), "No VODs for this matchup yet");
});

test("opening a guide from a VOD starts on the requested slide", () => {
  const h = harness("./MatchupGuideCarousel.tsx", "MatchupGuideCarousel");
  const tree = h.render({ ...props, initialSection: "mid-lane", guide: null, loading: false, abilities: {}, tier: "emerald_plus", matchup: null });
  assert.equal(text(nodes(tree).find(node => node.props.role === "tab" && node.props["aria-selected"])), "Mid lane");
  assert.equal(nodes(tree).filter(node => node.props.role === "tabpanel" && !node.props.hidden).length, 1);
});

test("general fallback is labelled above the existing carousel sections", () => {
  const guide = { champion: 103, lane: "mid", keyTips: [] };
  const h = harness("./MatchupGuideCarousel.tsx", "MatchupGuideCarousel");
  const tree = h.render({ ...props, guide, loading: false, abilities: {}, tier: "emerald_plus", matchup: null });
  assert.match(text(tree), /General Ahri guide \u00b7 no Zed matchup guide yet/);
  assert.equal(nodes(tree).find(n => n.type === "summary").props.guide, guide);
  assert.equal(nodes(tree).filter(n => n.type === "guide-content").length, 3);
});
