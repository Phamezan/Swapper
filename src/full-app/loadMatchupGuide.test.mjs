import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import ts from "typescript";

const fixture = { champion: 103, enemy: 238, lane: "mid", fixture: true };
function harness(dev, response, fails = false) {
  let fixtureLoads = 0;
  const calls = [];
  const source = readFileSync(new URL("./loadMatchupGuide.ts", import.meta.url), "utf8")
    .replaceAll("import.meta.env.DEV", String(dev));
  const code = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText;
  const exports = {};
  new Function("require", "exports", code)(name => {
    if (name === "@tauri-apps/api/core") return { invoke: async (command, args) => {
      calls.push({ command, args });
      if (fails) throw new Error("Unknown command get_matchup_guide");
      return response;
    } };
    if (name === "./fixtures/ahriVsZed") { fixtureLoads++; return { ahriVsZed: fixture }; }
    throw new Error(`Unexpected import: ${name}`);
  }, exports);
  return { load: exports.loadMatchupGuide, calls, fixtureLoads: () => fixtureLoads };
}

for (const fails of [false, true]) test(`development falls back for Ahri mid vs Zed on ${fails ? "IPC failure" : "null"}`, async () => {
  const h = harness(true, null, fails);
  assert.deepEqual(await h.load(103, 238, "mid"), { guide: fixture, record: { wins: 5, losses: 20 } });
  assert.equal(h.fixtureLoads(), 1);
  assert.deepEqual(h.calls, [{ command: "get_matchup_guide", args: { championId: 103, enemyChampionId: 238, position: "mid" } }]);
});

test("development fills a missing guide while preserving the player's actual record", async () => {
  const h = harness(true, { guide: null, record: { wins: 0, losses: 0 } });
  assert.deepEqual(await h.load(103, 238, "mid"), { guide: fixture, record: { wins: 0, losses: 0 } });
});

test("other champions, opponents, lanes and the reverse pair never load the fallback", async () => {
  for (const fails of [false, true]) for (const pair of [[103, 157, "mid"], [7, 238, "mid"], [103, 238, "top"], [238, 103, "mid"]]) {
    const h = harness(true, null, fails);
    assert.equal(await h.load(...pair), null);
    assert.equal(h.fixtureLoads(), 0);
  }
});

test("successful guide responses take precedence in development and production", async () => {
  const response = { guide: { champion: 103, enemy: 238, lane: "mid", reviewed: true }, record: { wins: 2, losses: 1 } };
  for (const dev of [false, true]) {
    const h = harness(dev, response);
    assert.equal(await h.load(103, 238, "mid"), response);
    assert.equal(h.fixtureLoads(), 0);
  }
});

test("production never imports the fixture on missing, failed or fixture-tagged responses", async () => {
  for (const [response, fails] of [[null, false], [null, true], [{ guide: null, record: null }, false], [{ guide: fixture, record: null }, false]]) {
    const h = harness(false, response, fails);
    const result = await h.load(103, 238, "mid");
    assert.equal(result?.guide ?? null, null);
    assert.equal(h.fixtureLoads(), 0);
  }
});
