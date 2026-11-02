import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import ts from "typescript";
import { adjacentMoment, currentMomentIndex, formatVodTime, groupMarkers, playerKeyAction, vodUrl } from "./matchupVodModel.ts";

test("VOD timestamps use minutes, pad seconds, and support hour-long games", () => {
  assert.deepEqual([0, 9, 61.9, 184, 3605, -1, NaN].map(formatVodTime), ["0:00", "0:09", "1:01", "3:04", "1:00:05", "0:00", "0:00"]);
});

test("real video links use a YouTube watch URL with an integer timestamp", () => {
  assert.equal(vodUrl({ videoId: "A_b-cD12345" }, 184.9), "https://www.youtube.com/watch?v=A_b-cD12345&t=184");
  assert.equal(vodUrl({ videoId: "A_b-cD12345" }), "https://www.youtube.com/watch?v=A_b-cD12345&t=0");
  for (const at of [-1, Infinity, NaN]) assert.equal(vodUrl({ videoId: "A_b-cD12345" }, at), null);
  for (const videoId of ["fixture-01", "", "../watch", "https://evil.io"]) assert.equal(vodUrl({ videoId }), null);
  assert.equal(vodUrl({ videoId: "A_b-cD12345", fixture: true }), null);
});

function loader(dev, response = [], fails = false) {
  let imports = 0;
  const calls = [];
  const fixture = [{ videoId: "fixture-ahri-1", fixture: true }];
  const source = readFileSync(new URL("./loadMatchupVods.ts", import.meta.url), "utf8").replaceAll("import.meta.env.DEV", String(dev));
  const code = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText;
  const exports = {};
  new Function("require", "exports", code)(name => {
    if (name === '@tauri-apps/api/core') return { invoke: async (command, args) => { calls.push({ command, args }); if (fails) throw new Error('offline'); return response; } };
    assert.equal(name, "./fixtures/ahriVsZedVods"); imports++; return { ahriVsZedVods: fixture };
  }, exports);
  return { load: exports.loadMatchupVods, imports: () => imports, fixture, calls };
}

test("only the exact Ahri vs Zed mid pair loads VOD preview data in development", async () => {
  const h = loader(true);
  assert.equal(await h.load(103, 238, "mid"), h.fixture);
  for (const args of [[103, 157, "mid"], [238, 103, "mid"], [103, 238, "top"], [0, 238, "mid"]]) assert.deepEqual(await h.load(...args), []);
  assert.equal(h.imports(), 1);
});

test("production never imports or returns preview VODs", async () => {
  const h = loader(false);
  assert.deepEqual(await h.load(103, 238, "mid"), []);
  assert.equal(h.imports(), 0);
});

test('cached VODs take precedence over development fixtures and invoke uses the matchup key', async () => {
  const real = [{ videoId: 'A_b-cD12345', moments: [] }];
  const h = loader(true, real);
  assert.equal(await h.load(103, 238, 'mid'), real);
  assert.deepEqual(h.calls, [{ command: 'get_matchup_vods', args: { championId: 103, enemyChampionId: 238, position: 'mid' } }]);
  assert.equal(h.imports(), 0);
  assert.deepEqual(await loader(false, [], true).load(103, 238, 'mid'), []);
  const fallback = loader(true, [], true);
  assert.equal(await fallback.load(103, 238, 'mid'), fallback.fixture);
});

test("every bundled VOD is visibly fictional, unplayable, and has valid guide chapters", async () => {
  const { ahriVsZedVods } = await import("./fixtures/ahriVsZedVods.ts");
  const { GUIDE_SECTIONS } = await import("./matchupGuide.ts");
  assert.equal(ahriVsZedVods.length, 5);
  assert.deepEqual(ahriVsZedVods.filter(vod => vod.team).map(vod => vod.team), ["T1", "Gen.G", "G2 Esports"]);
  assert.equal(ahriVsZedVods.filter(vod => !vod.team && vod.rankClaim).length, 2);
  assert.equal(new Set(ahriVsZedVods.map(vod => vod.videoId)).size, ahriVsZedVods.length);
  for (const vod of ahriVsZedVods) {
    assert.match(vod.player, /^Fixture /);
    assert.equal("side" in vod, false);
    assert.equal(vod.playerImage, undefined);
    assert.equal(vod.fixture, true);
    assert.equal(vod.patch, "26.21");
    assert.equal(vodUrl(vod), null);
    assert.ok(vod.moments.length);
    for (const moment of vod.moments) {
      assert.ok(moment.atSec >= 0 && moment.atSec < vod.durationSec);
      assert.ok(!moment.section || GUIDE_SECTIONS.some(([key]) => key === moment.section));
    }
  }
});

const moments = [184, 240, 242, 300, 900].map(atSec => ({ atSec, label: `m${atSec}` }));

test("the current moment is the latest one reached, tolerating a keyframe seek that lands just short", () => {
  assert.deepEqual([0, 183, 184, 239.2, 241, 898, 5000].map(time => currentMomentIndex(moments, time)), [-1, 0, 0, 1, 2, 3, 4]);
  assert.equal(currentMomentIndex([moments[3], moments[0]], 200), 1, "unsorted moments still pick by time");
  assert.equal(currentMomentIndex([], 100), -1);
});

test("previous and next moment skip the one playing, and previous restarts it only after a few seconds", () => {
  assert.equal(adjacentMoment(moments, 184, 1)?.atSec, 240);
  assert.equal(adjacentMoment(moments, 0, 1)?.atSec, 184);
  assert.equal(adjacentMoment(moments, 900, 1), undefined);
  assert.equal(adjacentMoment(moments, 301, -1)?.atSec, 242, "just after a moment, previous goes to the one before");
  assert.equal(adjacentMoment(moments, 310, -1)?.atSec, 300, "well into a moment, previous restarts it");
  assert.equal(adjacentMoment(moments, 185, -1), undefined);
});

test("markers closer than the gap merge into one group anchored at its earliest moment", () => {
  assert.deepEqual(groupMarkers(moments, 10), [{ atSec: 184, indices: [0] }, { atSec: 240, indices: [1, 2] }, { atSec: 300, indices: [3] }, { atSec: 900, indices: [4] }]);
  assert.deepEqual(groupMarkers(moments, 70).map(group => group.indices), [[0, 1, 2], [3], [4]], "groups do not chain past the gap from their first moment");
  assert.deepEqual(groupMarkers([moments[2], moments[0], moments[1]], 10).map(group => group.indices), [[1], [2, 0]]);
  assert.deepEqual(groupMarkers(moments, 0).length, 5);
});

test("player shortcuts follow YouTube and never take keys from fields or modifier combos", () => {
  const key = (key, target = { tagName: "DIV" }, extra = {}) => playerKeyAction({ key, target, ...extra });
  assert.deepEqual([" ", "k", "K", "m", "f", "j", "l", "ArrowLeft", "ArrowRight"].map(k => key(k)),
    ["toggle", "toggle", "toggle", "mute", "fullscreen", { seekBy: -10 }, { seekBy: 10 }, { seekBy: -5 }, { seekBy: 5 }]);
  for (const target of [{ tagName: "INPUT" }, { tagName: "textarea" }, { tagName: "SELECT" }, { tagName: "DIV", isContentEditable: true }]) assert.equal(key("k", target), null);
  assert.equal(key(" ", { tagName: "BUTTON" }), null, "Space presses the focused button instead");
  assert.equal(key("k", { tagName: "BUTTON" }), "toggle");
  for (const modifier of ["ctrlKey", "metaKey", "altKey"]) assert.equal(key("f", undefined, { [modifier]: true }), null);
  for (const other of ["a", "Escape", "Enter", "toString", "constructor"]) assert.equal(key(other), null);
  assert.equal(key("k", null), "toggle");
});
