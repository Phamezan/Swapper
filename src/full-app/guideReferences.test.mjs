import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import ts from "typescript";

function harness(file, name, command, cached = new Map()) {
  const states = [], effects = [], calls = [];
  let cursor = 0, pending = [], props, tree;
  const hooks = {
    useState(initial) {
      const i = cursor++;
      if (!(i in states)) states[i] = typeof initial === "function" ? initial() : initial;
      return [states[i], value => { states[i] = typeof value === "function" ? value(states[i]) : value; }];
    },
    useEffect(fn, deps) {
      const i = cursor++;
      if (!effects[i] || deps.some((value, j) => !Object.is(value, effects[i].deps[j]))) pending.push(() => {
        effects[i]?.cleanup?.(); effects[i] = { deps, cleanup: fn() };
      });
    },
  };
  const jsx = (type, props) => ({ type, props });
  const modules = {
    react: hooks, "react/jsx-runtime": { jsx, jsxs: jsx, Fragment: "fragment" },
    "@tauri-apps/api/core": { invoke: async (name, args) => { calls.push({ name, args }); return command(name, args); } },
    "../runes/iconLoader": {
      cachedIcon: key => cached.get(key),
      loadIcon: async (key, resolve) => {
        if (cached.has(key)) return cached.get(key);
        const value = await resolve(); cached.set(key, value); return value;
      },
    },
    "./ui/Slot": { Slot: "slot" }, "./ui/States": { Notice: "notice" },
    "./GuideSourceButton": { GuideSourceButton: "source-button" }, "./GuideItem": { GuideItem: "guide-item" },
    "./GuideTipText": { GuideTipText: "guide-text" }, "./AbilityHover": { AbilityHover: "ability" },
    "./MatchupGuideContent": { GuideSourceChip: "source-chip" },
    "radix-ui": { Popover: { Root: "popover-root", Trigger: "popover-trigger", Portal: "popover-portal", Content: "popover-content" } },
  };
  const code = ts.transpileModule(readFileSync(new URL(file, import.meta.url), "utf8"), {
    compilerOptions: { module: ts.ModuleKind.CommonJS, jsx: ts.JsxEmit.ReactJSX, target: ts.ScriptTarget.ES2022 },
  }).outputText;
  const exports = {};
  new Function("require", "exports", code)(key => modules[key] ?? {}, exports);
  const render = value => { props = value ?? props; cursor = 0; tree = exports[name](props); pending.splice(0).forEach(fn => fn()); return tree; };
  return { render, calls, async flush() { for (let i = 0; i < 5; i++) await new Promise(setImmediate); return render(); } };
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

test("source activation uses the Tauri command through a keyboard-accessible button", async () => {
  const h = harness("./GuideSourceButton.tsx", "GuideSourceButton", () => null);
  const tree = h.render({ url: "https://www.mobafire.com/league-of-legends/ahri-guide", label: "Author Name", children: "AN" });
  const button = nodes(tree).find(node => node.type === "button");
  assert.equal(button.props.type, "button");
  assert.equal(button.props["aria-label"], "Author Name");
  assert.equal(button.props.title, "Author Name");
  await button.props.onClick();
  assert.deepEqual(h.calls, [{ name: "open_guide_source", args: { url: "https://www.mobafire.com/league-of-legends/ahri-guide" } }]);
});

test("a rejected source URL surfaces the command error", async () => {
  const h = harness("./GuideSourceButton.tsx", "GuideSourceButton", () => { throw new Error("Host is not allowed"); });
  await nodes(h.render({ url: "https://evil.io", children: "Author" })).find(node => node.type === "button").props.onClick();
  assert.match(text(h.render()), /Host is not allowed/);
});

test("guide item references render nothing while unresolved and after unknown-id failure", async () => {
  const h = harness("./GuideItem.tsx", "GuideItem", () => { throw new Error("Unknown item"); });
  assert.equal(h.render({ id: 999999 }), null);
  assert.equal(await h.flush(), null);
  assert.deepEqual(h.calls, [{ name: "item_icon", args: { id: 999999 } }]);
});

test("known guide items reuse the house Slot after the shared icon resolves", async () => {
  const h = harness("./GuideItem.tsx", "GuideItem", () => "data:image/png;base64,known");
  assert.equal(h.render({ id: 2420 }), null);
  const tree = await h.flush();
  assert.equal(tree.type, "slot");
  assert.equal(tree.props.id, 2420);
  assert.equal(tree.props.kind, "item");
  assert.equal(h.render({ id: 999999 }), null, "an old item's icon must not flash for a new id");
});

test("cached guide items render immediately and invalid ids do not request art", async () => {
  const cached = harness("./GuideItem.tsx", "GuideItem", () => { throw new Error("Unexpected request"); }, new Map([["item:2420", "cached"]]));
  assert.equal(cached.render({ id: 2420 }).type, "slot");
  const invalid = harness("./GuideItem.tsx", "GuideItem", () => { throw new Error("Unexpected request"); });
  assert.equal(invalid.render({ id: 0 }), null);
  await invalid.flush();
  assert.deepEqual(invalid.calls, []);
});

test("tip authors precede the numbering in a separate gutter and keep the source action", () => {
  const source = { id: "author", author: "Author", rankClaim: "Master", url: "https://www.mobafire.com/league-of-legends/ahri-guide" };
  const tips = harness("./MatchupGuideContent.tsx", "GuideTips", () => null);
  const tree = tips.render({ tips: [{ text: "A tip", scope: "enemy", sourceIds: ["author"] }], sources: [source] });
  assert.doesNotMatch(text(tree), /Against this enemy|This pair/);
  const row = nodes(tree).find(node => node.type === "li");
  const [gutter, number, content] = row.props.children;
  assert.equal(gutter.props.className, "mg-tip-authors");
  assert.equal(number.props.className, "mg-tip-number");
  assert.equal(text(number), "01");
  assert.equal(content.props.className, "mg-tip-content");
  assert.equal(nodes(content).some(node => node.type === "source-button"), false);
  const credit = nodes(gutter).find(node => node.type === "source-button");
  assert.equal(credit.props.url, source.url);
  assert.equal(credit.props.label, source.author);
  const chips = harness("./MatchupGuideContent.tsx", "GuideSourceChip", () => null);
  const chip = chips.render({ source });
  assert.equal(chip.type, "source-button");
  assert.equal(chip.props.url, source.url);
});

test("tip gutters stack several sources in credit order and stay empty when no source resolves", () => {
  const sources = [
    { id: "a", author: "Lane Lab", url: "https://www.mobafire.com/a" },
    { id: "b", author: "Nine Tails", url: "https://www.mobafire.com/b" },
  ];
  const h = harness("./MatchupGuideContent.tsx", "GuideTips", () => null);
  const tree = h.render({ tips: [
    { text: "Several", sourceIds: ["b", "missing", "a"], patch: "26.21" },
    { text: "None" }, { text: "Unresolved", sourceIds: ["missing"] },
  ], sources });
  const rows = nodes(tree).filter(node => node.type === "li");
  assert.deepEqual(nodes(rows[0].props.children[0]).filter(node => node.type === "source-button").map(node => node.props.label), ["Nine Tails", "Lane Lab"]);
  assert.deepEqual(nodes(rows[0]).filter(node => node.props.className === "mg-tip-author-initials").map(text), ["NT", "LL"]);
  assert.equal(text(nodes(rows[0]).find(node => node.props.className === "mg-tip-meta")), "Written 26.21");
  for (const row of rows.slice(1)) assert.equal(text(row.props.children[0]), "");
});

test("header sources popover counts and lists credited authors with the existing source chips", () => {
  const sources = [
    { id: "a", author: "Author A", rankClaim: "Master", url: "https://www.mobafire.com/guide" },
    { id: "b", author: "Author B", rankClaim: "Grandmaster", url: "https://docs.google.com/spreadsheets/d/example" },
  ];
  const h = harness("./GuideSourcesPopover.tsx", "GuideSourcesPopover", () => null);
  const tree = h.render({ sources });
  assert.equal(text(nodes(tree).find(node => node.type === "popover-trigger")), "Sources (2)");
  assert.equal(nodes(tree).find(node => node.type === "popover-content").props["aria-label"], "Guide sources");
  assert.deepEqual(nodes(tree).filter(node => node.type === "source-chip").map(node => node.props.source), sources);
  assert.equal(h.render({ sources: [] }), null);
});

test("a backend outdated flag renders a readable warning", () => {
  const h = harness("./MatchupGuideContent.tsx", "GuideTips", () => null);
  const tree = h.render({ tips: [{ text: "Hold Charm.", scope: "pair", outdated: true }], sources: [] });
  assert.match(text(tree), /May be outdated: a champion changed after this tip's patch\./);
});
