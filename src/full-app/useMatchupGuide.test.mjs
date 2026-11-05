import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import ts from "typescript";
import * as dataModel from "./matchupDataCard.ts";

function harness() {
  const states = [], effects = [], listeners = new Map(), requests = [], dataRequests = [], timers = new Map(), intervals = new Map(), focus = new Map();
  let timerId = 0;
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
    "./matchupDataCard": dataModel,
    "@tauri-apps/api/core": { invoke: (name, args) => name === "get_matchup_data_card" ? new Promise(resolve => dataRequests.push({ args, resolve })) : Promise.resolve([]) },
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
  new Function("require", "exports", "window", "setInterval", "clearInterval", "setTimeout", "clearTimeout", code)(
    key => modules[key] ?? {}, exports,
    { addEventListener: (name, fn) => focus.set(name, fn), removeEventListener: name => focus.delete(name) },
    (fn, ms) => { const id = ++timerId; intervals.set(id, { fn, ms }); return id; }, id => intervals.delete(id),
    (fn, ms) => { const id = ++timerId; timers.set(id, { fn, ms }); return id; }, id => timers.delete(id),
  );
  const render = next => { args = next ?? args; cursor = 0; const result = exports.useMatchupGuide(...args); pending.splice(0).forEach(fn => fn()); return result; };
  return { render, requests, dataRequests, listeners, focus, timers, intervals, async flush() { for (let i = 0; i < 4; i++) await new Promise(setImmediate); return render(); } };
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

test("general guides are accepted only for the requested champion and lane", async () => {
  for (const guide of [{ champion: 103, lane: "mid" }, { champion: 238, lane: "mid" }, { champion: 103, lane: "top" }]) {
    const h = harness(); h.render();
    h.requests[0].resolve({ guide, record: null });
    assert.equal((await h.flush()).guide, guide.champion === 103 && guide.lane === "mid" ? guide : null);
  }
});

const absence = guide => ({ guide, record: null, lookup: "absent", lookupExpiresAt: Date.now() + 6 * 3600e3 });
const candidate = (tier = "emerald_plus") => ({ kind: "data-based", championId: 103, enemyChampionId: 238, lane: "mid", tier,
  stats: { wins: 460, games: 1000, patch: "16.22", dataVersion: "1.5.0", provider: "ugg", region: "world", queue: "ranked_solo_5x5", tier, endpointUrl: "test", schemaId: "test", fetchedAt: Date.now(), stale: false },
  enemyKit: { version: "16.22", fetchedAt: Date.now(), assetUrl: "test", sheet: { championId: 238, spells: ["Q", "W"].map(key => ({ key, name: key, cooldowns: [20,18,16,14,12], ranges: [], charges: 0, recharge: [] })) } } });

test("unknown and expired lookup never request data; general remains visible during a confirmed data request", async () => {
  for (const value of [{ guide: null, record: null, lookup: "unknown" }, { ...absence(null), lookupExpiresAt: Date.now() - 1 }]) {
    const h = harness(); h.render(); h.requests[0].resolve(value);
    await h.flush(); assert.equal(h.dataRequests.length, 0);
    h.render([0,0,"mid"]);
  }
  const h = harness(), general = { champion: 103, lane: "mid" };
  h.render(); h.requests[0].resolve(absence(general));
  assert.equal((await h.flush()).guide, general);
  assert.equal(h.dataRequests.length, 1);
  h.dataRequests[0].resolve(candidate());
  assert.equal((await h.flush()).content.kind, "data-based");
  assert.equal(h.timers.size, 1);
  h.render([0,0,"mid"]); await h.flush();
  assert.equal(h.timers.size, 0); assert.equal(h.intervals.size, 0); assert.equal(h.focus.size, 0);
});

test("source arrival suppresses both completed and in-flight data and focus/timer revalidate the guide", async () => {
  for (const finishFirst of [false, true]) {
    const h = harness(); h.render(); h.requests[0].resolve(absence(null)); await h.flush();
    if (finishFirst) { h.dataRequests[0].resolve(candidate()); assert.equal((await h.flush()).content.kind, "data-based"); }
    h.focus.get("focus")();
    const exact = { champion: 103, enemy: 238, lane: "mid", reviewed: false };
    h.requests[1].resolve({ guide: exact, record: null, lookup: "exact" });
    assert.equal((await h.flush()).guide, exact);
    if (!finishFirst) { h.dataRequests[0].resolve(candidate()); assert.equal((await h.flush()).guide, exact); }
    assert.equal(h.timers.size, 0);
    const interval = [...h.intervals.values()][0]; assert.equal(interval.ms, 6 * 3600e3);
    interval.fn(); assert.equal(h.requests.length, 3);
    h.render([0,0,"mid"]);
  }
});

test("tier/role changes discard older data responses and unresolved same-role opponent disables data", async () => {
  const h = harness(); h.render(); h.requests[0].resolve(absence(null)); await h.flush();
  h.render([103,238,"mid","diamond_plus"]);
  h.requests[1].resolve(absence(null)); await h.flush();
  assert.equal(h.dataRequests[1].args.tier, "diamond_plus");
  h.dataRequests[0].resolve(candidate()); assert.equal((await h.flush()).content, null);
  h.dataRequests[1].resolve(candidate("diamond_plus")); assert.equal((await h.flush()).content.kind, "data-based");
  h.render([103,238,"support","diamond_plus"]); assert.equal(h.render().content, null);
  h.render([103,238,"mid","diamond_plus",false]);
  h.requests.at(-1).resolve(absence(null)); await h.flush();
  assert.equal(h.dataRequests.length, 2);
  h.render([0,0,"mid"]);
});

test("catalog expiry removes data and triggers a fresh sourced lookup", async () => {
  const h = harness(); h.render(); h.requests[0].resolve(absence(null)); await h.flush();
  h.dataRequests[0].resolve(candidate()); await h.flush();
  const expiry = [...h.timers.values()][0];
  assert.ok(expiry.ms <= 6 * 3600e3);
  expiry.fn();
  assert.equal(h.render().content, null);
  assert.equal(h.requests.length, 2);
  h.render([0,0,"mid"]);
});
