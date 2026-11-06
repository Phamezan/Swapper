import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import ts from "typescript";

function load() {
  const require = createRequire(import.meta.url);
  const code = ts.transpileModule(readFileSync(new URL("./ui/Tooltip.tsx", import.meta.url), "utf8"), {
    compilerOptions: { module: ts.ModuleKind.CommonJS, jsx: ts.JsxEmit.ReactJSX, target: ts.ScriptTarget.ES2022 },
  }).outputText;
  const exports = {};
  new Function("require", "exports", code)(name => name.endsWith(".css") ? {} : require(name), exports);
  return exports;
}

test("Tooltip renders the original trigger with its accessible name and no native title", () => {
  const { Tooltip, TooltipProvider } = load();
  const html = renderToStaticMarkup(createElement(TooltipProvider, null,
    createElement(Tooltip, { label: "Refresh stats" },
      createElement("button", { "aria-label": "Refresh stats" }, "Refresh"))));
  assert.match(html, /^<button /, "asChild preserves the original layout element");
  assert.match(html, /aria-label="Refresh stats"/);
  assert.match(html, /data-tooltip="Refresh stats"/);
  assert.doesNotMatch(html, / title=/);
  assert.match(html, />Refresh<\/button>$/);
});

test("Tooltip leaves an unlabelled child unchanged without requiring a provider", () => {
  const { Tooltip } = load();
  const child = createElement("span", null, "No hint");
  for (const label of [undefined, ""]) {
    assert.equal(renderToStaticMarkup(createElement(Tooltip, { label }, child)), "<span>No hint</span>");
  }
});

test("nested tooltip focus and hover do not open an ancestor tooltip", () => {
  const { Tooltip } = load();
  const trigger = Tooltip({ label: "Outer", children: createElement("div") }).props.children[0];
  const outer = {}, inner = {};
  for (const handler of [trigger.props.onFocus, trigger.props.onPointerMove]) {
    for (const nearest of [outer, inner]) {
      let prevented = false;
      handler({ currentTarget: outer, target: { closest: () => nearest }, preventDefault: () => { prevented = true; } });
      assert.equal(prevented, nearest === inner, "a descendant input still opens its enclosing tooltip");
    }
  }
});
