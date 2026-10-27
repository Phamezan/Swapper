import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import ts from "typescript";

function harness() {
  const states = [], effects = [], listeners = new Map(), requests = [];
  let cursor = 0, pending = [], args = [103, 238, "mid"];
  const hooks = {
    useState(initial) {
      const i = cursor++;
      if (!(i in states)) states[i] = initial;
      return [states[i], value => { states[i] = typeof value === "function" ? value(states[i]) : value; }];
    },
    useEffect(fn, deps) {
      const i = cursor++;
      if (!effects[i] || deps.some((value, j) => !Object.is(value, effects[i].deps[j]))) pending.push(() => {
        effects[i]?.cleanup?.(); effects[i] = { deps, cleanup: fn() };
      });
    },
  };
  const modules = {
    react: hooks,
    "@tauri-apps/api/core": { invoke: async () => [] },
    "@tauri-apps/api/event": { listen: async (name, callback) => {
      listeners.set(name, callback);
      return () => { if (listeners.get(name) === callback) listeners.delete(name); };
    } },
    "./loadMatchupGuide": { loadMatchupGuide: (...args) => new Promise(resolve => requests.push({ args, resolve })) },
  };
  const code = ts.transpileModule(readFileSync(new URL("./useMatchupGuide.ts", import.meta.url), "utf8"), {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
  }).outputText;
  const exports = {};
  new Function("require", "exports", code)(key => modules[key] ?? {}, exports);
  const render = next => { args = next ?? args; cursor = 0; const result = exports.useMatchupGuide(...args); pending.splice(0).forEach(fn => fn()); return result; };
  return { render, requests, listeners, async flush() { for (let i = 0; i < 4; i++) await new Promise(setImmediate); return render(); } };
}

test("record pushes refresh an open guide and older reads cannot replace the new record", async () => {
  const h = harness();
  h.render();
  await h.flush();
  assert.equal(typeof h.listeners.get("matchup_records_changed"), "function");
  h.listeners.get("matchup_records_changed")({ payload: null });
  assert.equal(h.requests.length, 2);
  h.requests[1].resolve({ guide: null, record: { wins: 5, losses: 20 } });
  assert.deepEqual((await h.flush()).record, { wins: 5, losses: 20 });
  h.requests[0].resolve({ guide: null, record: null });
  assert.deepEqual((await h.flush()).record, { wins: 5, losses: 20 });
  h.render([0, 0, "mid"]);
  await h.flush();
  assert.equal(h.listeners.size, 0);
});
