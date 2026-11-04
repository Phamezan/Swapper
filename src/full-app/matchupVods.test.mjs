import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import ts from "typescript";
import { ANY_VOD, facetCounts, isPlaceholderPhoto, adjacentMoment, comparePatch, currentMomentIndex, filterVods, formatVodTime, groupMarkers, idleTimer, playerKeyAction, rankTier, segmentOf, uploadAge, vodKey, vodMatches, vodRegion, vodRow, vodUrl } from "./matchupVodModel.ts";

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
  const zed = { videoId: "fixture-ahri-1", fixture: true, enemy: 238, lane: "mid" };
  const fixture = [zed, { videoId: "fixture-ahri-6", fixture: true, enemy: 157, lane: "mid" }, { videoId: "fixture-ahri-7", fixture: true, enemy: 238, lane: "top" }];
  const source = readFileSync(new URL("./loadMatchupVods.ts", import.meta.url), "utf8").replaceAll("import.meta.env.DEV", String(dev));
  const code = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText;
  const exports = {};
  new Function("require", "exports", code)(name => {
    if (name === '@tauri-apps/api/core') return { invoke: async (command, args) => { calls.push({ command, args }); if (fails) throw new Error('offline'); return response; } };
    assert.equal(name, "./fixtures/ahriChampionVods"); imports++; return { ahriChampionVods: fixture };
  }, exports);
  return { load: exports.loadMatchupVods, loadChampion: exports.loadChampionVods, loadAll: exports.loadAllVods, imports: () => imports, fixture, zed, calls };
}

test("only the exact Ahri vs Zed mid pair loads VOD preview data in development", async () => {
  const h = loader(true);
  assert.deepEqual(await h.load(103, 238, "mid"), [h.zed], "only the fixture's Ahri vs Zed mid games");
  for (const args of [[103, 157, "mid"], [238, 103, "mid"], [103, 238, "top"], [0, 238, "mid"]]) assert.deepEqual(await h.load(...args), []);
  assert.equal(h.imports(), 1);
  assert.equal(await h.loadChampion(103), h.fixture, "the champion-wide preview spans enemies and lanes");
  assert.deepEqual(await h.loadChampion(238), []);
  assert.deepEqual(h.calls.at(-1), { command: "get_champion_vods", args: { championId: 238 } });
});

test("production never imports or returns preview VODs", async () => {
  const h = loader(false);
  assert.deepEqual(await h.load(103, 238, "mid"), []);
  assert.deepEqual(await h.loadChampion(103), []);
  assert.equal(h.imports(), 0);
});

test('cached VODs take precedence over development fixtures and invoke uses the matchup key', async () => {
  const real = [{ videoId: 'A_b-cD12345', moments: [] }];
  const h = loader(true, real);
  assert.equal(await h.load(103, 238, 'mid'), real);
  assert.deepEqual(h.calls, [{ command: 'get_matchup_vods', args: { championId: 103, enemyChampionId: 238, position: 'mid' } }]);
  assert.equal(h.imports(), 0);
  assert.deepEqual(await loader(false, [], true).load(103, 238, 'mid'), null);
  assert.equal(await loader(true, [], true).load(103, 238, "mid"), null);
  const fallback = loader(true, []);
  assert.deepEqual(await fallback.load(103, 238, 'mid'), [fallback.zed]);
  assert.equal(await loader(true, real).loadChampion(103), real);
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
  assert.deepEqual(["[", "]"].map(k => key(k)), [{ moment: -1 }, { moment: 1 }], "[ and ] step through key moments");
});

test("upload age reads as days or weeks ago when recent and as a short date after a month", () => {
  const now = new Date(2026, 10, 2, 23, 30);
  assert.deepEqual(["2026-11-02", "2026-11-01", "2026-10-30", "2026-10-19", "2026-08-01", "2026-12-25"].map(day => uploadAge(day, now)),
    ["today", "yesterday", "3 days ago", "2 weeks ago", "Aug 1, 2026", "Dec 25, 2026"]);
  for (const bad of [undefined, "", "2026-8-1", "yesterday", "2026-13-45"]) assert.equal(uploadAge(bad, now), null);
});

test("the champion preview library is large, varied, unplayable and never reuses an ID", async () => {
  const { ahriChampionVods } = await import("./fixtures/ahriChampionVods.ts");
  assert.ok(ahriChampionVods.length >= 1000);
  assert.equal(new Set(ahriChampionVods.map(vod => vod.videoId)).size, ahriChampionVods.length);
  assert.ok(ahriChampionVods.every(vod => vod.fixture && vodUrl(vod) === null && vod.enemy > 0 && vod.lane && vod.moments.every(m => m.atSec < vod.durationSec)));
  for (const values of [vod => rankTier(vod.rankClaim), vod => vod.result, vod => vod.patch, vod => vod.enemy, vod => vod.lane, vod => !!vod.team, vod => !!vod.player])
    assert.ok(new Set(ahriChampionVods.map(values)).size > 1);
});

test("rank tiers come from free-text claims, case-insensitively, and grandmaster is never read as master", () => {
  assert.deepEqual(["Challenger", "KR Challenger", "challenger", "KR Grandmaster", "GRANDMASTER", "Grand Master", "EUW GM", "MASTER", "Master 400 LP", "Diamond 1", "", "Gamma"].map(rankTier),
    ["challenger", "challenger", "challenger", "grandmaster", "grandmaster", "grandmaster", "grandmaster", "master", "master", "other", "other", "other"]);
});

test("patches compare numerically, not as text", () => {
  assert.ok(comparePatch("26.10", "26.9") > 0);
  assert.ok(comparePatch("26.21.1", "26.21") > 0);
  assert.equal(comparePatch("26.21", "26.21.0"), 0);
  assert.ok(comparePatch("25.24", "26.1") < 0);
});

const library = [
  { videoId: "a", rankClaim: "KR Challenger", team: "T1", result: "W", patch: "26.9", durationSec: 1500, enemy: 238 },
  { videoId: "b", rankClaim: "Grandmaster", result: "L", patch: "26.10", durationSec: 1200, enemy: 157 },
  { videoId: "c", rankClaim: "MASTER", patch: "26.10", durationSec: 1800, enemy: 238 },
  { videoId: "d", rankClaim: "Diamond", team: "G2", result: "W", patch: "26.21", durationSec: 1300, enemy: 7 },
  { videoId: "e", rankClaim: "challenger", result: "W", patch: "26.21", durationSec: 1100, enemy: 238 },
];
const ids = (filters) => filterVods(library, { ...ANY_VOD, ...filters }).map(vod => vod.videoId);

test("VODs always list the newest patch first, keeping content order for ties", () => {
  assert.deepEqual(ids({}), ["d", "e", "b", "c", "a"]);
  assert.deepEqual(library.map(vod => vod.videoId), ["a", "b", "c", "d", "e"], "sorting never reorders the source list");
});

test("filters combine named players, rank, patch, opponent and lane", () => {
  const list = library.map((vod, i) => ({ ...vod, player: i === 0 ? 'Faker' : i === 2 ? ' ? ' : undefined, lane: i === 0 ? 'mid' : 'top' }));
  const matching = filters => filterVods(list, { ...ANY_VOD, ...filters }).map(v => v.videoId);
  assert.deepEqual(matching({ named: true }), ['a']);
  assert.deepEqual(matching({ enemy: 238, named: true, tier: 'challenger', lane: 'mid' }), ['a']);
  assert.deepEqual(matching({ tier: 'other' }), ['d']);
  assert.deepEqual(matching({ patch: '26.10' }), ['b', 'c']);
  assert.deepEqual(matching({ enemy: 238 }), ['e', 'c', 'a']);
  assert.deepEqual(matching({ patch: '26.1' }), []);
});

test("the HUD idle timer hides after the delay, wakes on activity and restarts its countdown", () => {
  const timers = new Map(); let next = 0;
  const clock = { setTimeout: (fn, ms) => { timers.set(++next, { fn, ms }); return next; }, clearTimeout: id => timers.delete(id) };
  const changes = [];
  const idle = idleTimer(3000, value => changes.push(value), clock);
  assert.deepEqual([...timers.values()].map(t => t.ms), [3000], "counting starts immediately");
  idle.activity(); idle.activity();
  assert.equal(timers.size, 1, "activity replaces the countdown instead of stacking timers");
  assert.deepEqual(changes, [], "activity while visible changes nothing");
  [...timers.values()][0].fn();
  assert.deepEqual(changes, [true]);
  idle.activity();
  assert.deepEqual(changes, [true, false], "the next activity shows the HUD again");
  assert.equal(timers.size, 1);
  idle.stop();
  assert.equal(timers.size, 0);
});

test("the server region is read next to the rank, from the claim first and then the title", () => {
  const region = (title, rankClaim = "Challenger") => vodRegion({ title, rankClaim });
  assert.equal(region("(Peng) Ahri Mid vs Akali - EUW Challenger Patch 26.19"), "EUW");
  assert.equal(region("Kayle vs Sylas Mid - EUNE Challenger - Patch 16.10 Season 16"), "EUNE");
  assert.equal(region("Ahri Top vs Jayce - KR CHallenger Patch 26.16"), "KR");
  assert.equal(region("Ahri vs Zed - kr1 grandmaster"), "KR");
  assert.equal(region("Ahri vs Zed", "NA Master"), "NA", "the rank claim names the region");
  assert.equal(region("Ahri vs Zed - EUW Master", "KR Master"), "KR", "the claim wins over the title");
  for (const title of ["Ahri Mid vs Master Yi - Challenger", "(KR) Ahri vs Zed", "(EUW Peng) Ahri vs Zed - Patch 26.1", "NAME Master guide", "Ahri vs Zed"])
    assert.equal(region(title), null, title);
});

test("list rows lead with the player, or rank and region, and leave out an unknown result and zero moments", () => {
  const base = { videoId: "A_b-cD12345", title: "(Peng) Ahri Mid vs Akali - EUW Challenger Patch 26.19", rankClaim: "Challenger", patch: "26.19", durationSec: 1700 };
  assert.deepEqual(vodRow({ ...base, player: "Peng", team: "MKOI", result: "W", moments: [{ atSec: 1, label: "a" }, { atSec: 2, label: "b" }] }),
    { rank: "Challenger", region: "EUW", crest: "challenger", patch: "26.19", segment: null, primary: "Peng", named: true, details: ["EUW Challenger", "Patch 26.19"], result: "win", moments: 2 });
  assert.deepEqual(vodRow({ ...base, title: "Ahri Mid vs Akali - KR Master Patch 26.19", rankClaim: "MASTER", player: "?", moments: [] }),
    { rank: "Master", region: "KR", crest: "master", patch: "26.19", segment: null, primary: "KR Master", named: false, details: ["Patch 26.19"], result: null, moments: null });
  assert.equal(vodRow({ ...base, result: "L", moments: [] }).result, "loss");
  assert.deepEqual(vodRow({ ...base, title: "Ahri vs Zed", rankClaim: "KR Diamond 1", moments: [] }).primary, "KR Diamond 1", "an unknown tier keeps its own words without repeating the region");
  assert.deepEqual(vodRow({ ...base, title: "Ahri vs Zed", rankClaim: " ", player: "Faker", moments: [] }).details, ["Patch 26.19"]);
  assert.equal(vodRow({ ...base, title: "Ahri vs Zed", rankClaim: "", moments: [] }).primary, "Ranked game");
});

test("a dark placeholder silhouette is not shown as a player photo", () => {
  const pixels = (count, rgba) => Array.from({ length: count }, () => rgba).flat();
  // Measured: real headshots are 26%+ bright pixels, CDN silhouettes 8% or fewer.
  assert.equal(isPlaceholderPhoto([...pixels(70, [20, 20, 24, 255]), ...pixels(30, [230, 220, 210, 255])]), false, "a real headshot");
  assert.equal(isPlaceholderPhoto([...pixels(92, [10, 10, 12, 255]), ...pixels(8, [200, 200, 210, 255])]), true, "a rim-lit silhouette");
  assert.equal(isPlaceholderPhoto([...pixels(50, [255, 255, 255, 0]), ...pixels(50, [8, 8, 8, 255])]), true, "transparent pixels are ignored");
  assert.equal(isPlaceholderPhoto(pixels(10, [255, 255, 255, 0])), true, "an empty image");
});


test("library sorts patch, named player, upload day and stable ties", () => {
  const base = { rankClaim: "Master", moments: [], patch: "26.21" };
  const list = [
    { ...base, videoId: "anonymous-new", uploaded: "2026-11-02", player: "?", team: "T1" },
    { ...base, videoId: "named-old", player: "Faker", uploaded: "2026-10-01" },
    { ...base, videoId: "named-new", player: "Peyz", uploaded: "2026-11-01" },
    { ...base, videoId: "tie", player: "Ruler", uploaded: "2026-11-01" },
    { ...base, videoId: "new-patch", patch: "26.22" },
  ];
  assert.deepEqual(filterVods(list, ANY_VOD).map(v => v.videoId), ["new-patch", "named-new", "tie", "named-old", "anonymous-new"]);
  assert.deepEqual(filterVods(list, { ...ANY_VOD, named: true }).map(v => v.videoId), ["named-new", "tie", "named-old"]);
});

test("explicit region wins and estimated patches carry a tilde", () => {
  const vod = { videoId: "x", title: "EUW Challenger", rankClaim: "Grandmaster", region: "KR", patch: "26.21", patchEstimated: true, moments: [] };
  assert.equal(vodRegion(vod), "KR");
  assert.equal(vodRow(vod).primary, "KR Grandmaster");
  assert.deepEqual(vodRow(vod).details, ["Patch ~26.21"]);
});


test("all-VOD loader invokes the aggregate endpoint and only development adds tagged preview rows", async () => {
  const real = [{videoId:'A_b-cD12345',champion:238,enemy:103,lane:'mid',moments:[]}];
  const cached = loader(false, real);
  assert.equal(await cached.loadAll(), real);
  assert.deepEqual(cached.calls, [{command:'get_all_vods',args:undefined}]);
  assert.equal(cached.imports(), 0);
  assert.deepEqual(await loader(false, [], true).loadAll(), null);
  assert.equal(await loader(true, [], true).loadAll(), null);
  const preview = loader(true, []);
  assert.deepEqual(await preview.loadAll(), preview.fixture.map(vod => ({...vod,champion:103})));
});

test("vodKey and segmentOf default to the full video and vodUrl opens at the segment start", () => {
  assert.equal(vodKey({ videoId: "A_b-cD12345" }), "A_b-cD12345@0");
  assert.equal(vodKey({ videoId: "A_b-cD12345", startSec: 5025 }), "A_b-cD12345@5025");
  assert.deepEqual(segmentOf({ durationSec: 1746 }), { start: 0, end: 1746 });
  assert.deepEqual(segmentOf({ durationSec: 10800, startSec: 5025, endSec: 7510 }), { start: 5025, end: 7510 });
  assert.equal(vodUrl({ videoId: "A_b-cD12345", startSec: 5025 }), "https://www.youtube.com/watch?v=A_b-cD12345&t=5025");
  assert.equal(vodUrl({ videoId: "A_b-cD12345", startSec: 5025 }, 6000), "https://www.youtube.com/watch?v=A_b-cD12345&t=6000", "an explicit moment time wins");
});

test("the streamer filter keeps only stream VODs and an opponent filter excludes vs-any VODs", () => {
  const list = [
    { videoId: "s", rankClaim: "Master", patch: "26.21", durationSec: 3600, moments: [], format: "stream", cam: true, startSec: 100, endSec: 900 },
    { videoId: "r", rankClaim: "Master", patch: "26.21", durationSec: 1500, moments: [], enemy: 238 },
    { videoId: "a", rankClaim: "Master", patch: "26.21", durationSec: 1500, moments: [], format: "stream" },
  ];
  assert.deepEqual(filterVods(list, { ...ANY_VOD, streamer: true }).map(v => v.videoId), ["s", "a"]);
  assert.deepEqual(filterVods(list, { ...ANY_VOD, enemy: 238 }).map(v => v.videoId), ["r"], "vs any has no enemy to match");
  assert.equal(vodMatches(list[2], ANY_VOD), true);
  assert.equal(segmentOf(list[0]).end - segmentOf(list[0]).start, 800);
});

test('facet counts equal brute force for every selection including unknown regions', () => {
  const pool = Array.from({length:40}, (_,i) => ({videoId:String(i),player:i%2?'Player':undefined,format:i%3?'replay':'stream',rankClaim:i%2?'Master':'Diamond',patch:i%3?'26.21':'26.22',enemy:i%2?238:157,lane:i%3?'mid':'top',region:i%4?'KR':undefined,title:'Game',moments:[]}));
  for (const f of [ANY_VOD, {...ANY_VOD,named:true,tier:'master'}, {...ANY_VOD,region:'KR',enemy:238,streamer:true}]) {
    const counts=facetCounts(pool,f);
    assert.equal(counts.total,pool.filter(v=>vodMatches(v,f)).length);
    for (const key of ['named','streamer','tier','patch','enemy','lane','region']) {
      const values = key==='named'||key==='streamer' ? [false,true] : [null,...counts[key].keys()];
      for(const value of values) assert.equal(counts[key].get(value===false?null:value)??0,pool.filter(v=>vodMatches(v,{...f,[key]:value})).length, key);
    }
  }
  assert.equal(filterVods(pool,{...ANY_VOD,region:'NA'}).length,0);
});
