import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import ts from "typescript";
import { formatVodTime, vodUrl } from "./matchupVodModel.ts";

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

function loader(dev) {
  let imports = 0;
  const fixture = [{ videoId: "fixture-ahri-1", fixture: true }];
  const source = readFileSync(new URL("./loadMatchupVods.ts", import.meta.url), "utf8").replaceAll("import.meta.env.DEV", String(dev));
  const code = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText;
  const exports = {};
  new Function("require", "exports", code)(name => {
    assert.equal(name, "./fixtures/ahriVsZedVods"); imports++; return { ahriVsZedVods: fixture };
  }, exports);
  return { load: exports.loadMatchupVods, imports: () => imports, fixture };
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
