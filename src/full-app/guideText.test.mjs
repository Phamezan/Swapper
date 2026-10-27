import { test } from "node:test";
import assert from "node:assert/strict";
import { parseGuideText, liveCooldown } from "./guideText.ts";

test("cooldown tokens preserve prose and identify each champion and ability", () => {
  assert.deepEqual(parseGuideText("W: {cd:238:W}; E: {cd:103:E}."), [
    { text: "W: " }, { champion: 238, key: "W" }, { text: "; E: " }, { champion: 103, key: "E" }, { text: "." },
  ]);
  assert.deepEqual(parseGuideText("{cd:238:W}{cd:103:E}"), [{ champion: 238, key: "W" }, { champion: 103, key: "E" }]);
  assert.deepEqual(parseGuideText("{cd:0:Q} {cd:238:P} {cd:zed:W} {cd:238:W"), [{ text: "{cd:0:Q} {cd:238:P} {cd:zed:W} {cd:238:W" }]);
});

test("cooldown labels follow live per-rank values, including charge recharge", () => {
  const ability = { name: "Living Shadow", cooldowns: [20, 18.5, 17, 15.5, 14], charges: 0, recharge: [] };
  assert.equal(liveCooldown(ability, "W"), "20/18.5/17/15.5/14s");
  assert.equal(liveCooldown({ ...ability, cooldowns: [18, 17, 16, 15, 14] }, "W"), "18/17/16/15/14s");
  assert.equal(liveCooldown({ ...ability, cooldowns: [12, 12, 12] }, "W"), "12s");
  assert.equal(liveCooldown({ ...ability, charges: 2, recharge: [9, 8, 7] }, "W"), "9/8/7s");
});

test("unavailable or invalid cooldowns show only the ability name", () => {
  assert.equal(liveCooldown(undefined, "Living Shadow"), "Living Shadow");
  for (const cooldowns of [[], [0, 0], [NaN], [Infinity], [-1]]) {
    assert.equal(liveCooldown({ name: "Charm", cooldowns, charges: 0, recharge: [] }, "E"), "Charm");
  }
});
