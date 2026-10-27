import { test } from "node:test";
import assert from "node:assert/strict";
import { cooldownRows, cooldownTicks, windowAbility, spikeMarkers } from "./matchupVisuals.ts";

const spell = (key, cooldowns, extras = {}) => ({ key, name: `Ability ${key}`, iconPath: null, cooldowns, charges: 0, recharge: [], ...extras });

test("cooldown rows keep all basic ranks, cap R at three, and use recharge for charges", () => {
  const sheet = { spells: [spell("R", [120, 100, 80, 60, 40]), spell("E", [4], { charges: 2, recharge: [20, 18, 16, 14, 12] }), spell("Q", [10, 9, 8, 7, 6]), spell("W", [12])] };
  const rows = cooldownRows(sheet);
  assert.deepEqual(rows.map(row => [row.key, row.seconds, row.ranks.length, row.recharge]), [["Q", 10, 5, false], ["W", 12, 5, false], ["E", 20, 5, true], ["R", 120, 3, false]]);
  assert.deepEqual(rows.find(row => row.key === "R").ranks, [{ rank: 1, seconds: 120 }, { rank: 2, seconds: 100 }, { rank: 3, seconds: 80 }]);
  assert.equal(cooldownTicks(rows.find(row => row.key === "E").ranks, 20)[3].seconds, 12);
  assert.deepEqual(cooldownRows(null), []);
  assert.deepEqual(cooldownRows({ spells: [spell("Q", []), spell("W", [NaN]), spell("E", [0]), spell("R", [-1])] }), []);
});

test("rank ticks use the shared axis and group ranks with identical cooldowns", () => {
  const [row] = cooldownRows({ spells: [spell("W", [20, 18.5, 17, 15.5, 14])] });
  assert.deepEqual(cooldownTicks(row.ranks, 20), [
    { ranks: [2], seconds: 18.5, percent: 92.5 }, { ranks: [3], seconds: 17, percent: 85 },
    { ranks: [4], seconds: 15.5, percent: 77.5 }, { ranks: [5], seconds: 14, percent: 70 },
  ]);
  const [constant] = cooldownRows({ spells: [spell("Q", [7])] });
  assert.deepEqual(cooldownTicks(constant.ranks, 20), [{ ranks: [2, 3, 4, 5], seconds: 7, percent: 35 }]);
});

test("rank ticks preserve available ranks, omit invalid values, and allow cooldowns that increase", () => {
  const [row] = cooldownRows({ spells: [spell("Q", [5, NaN, 0, 8, -1])] });
  assert.deepEqual(cooldownTicks(row.ranks, 10), [{ ranks: [4], seconds: 8, percent: 80 }]);
  assert.deepEqual(cooldownTicks([], 10), []);
});

test("trade window chooses the longest enemy basic cooldown, excludes R and breaks ties in Q/W/E order", () => {
  const rows = cooldownRows({ spells: [spell("R", [120]), spell("E", [20]), spell("W", [20]), spell("Q", [6])] });
  assert.equal(windowAbility(rows).key, "W");
  assert.equal(windowAbility(rows.filter(row => row.key === "R")), null);
  assert.equal(windowAbility([]), null);
});

test("spike markers keep level checkpoints separate from the first three item purchases", () => {
  const items = [{ id: 3118, name: "Malignance" }, { id: 3157, name: "Zhonya’s Hourglass" }, { id: 3020, name: "Sorcerer’s Shoes" }, { id: 3089, name: "Deathcap" }];
  const markers = spikeMarkers(items);
  assert.deepEqual(markers.filter(marker => marker.kind === "level").map(marker => [marker.level, marker.rank]), [[6, 1], [11, 2], [16, 3]]);
  assert.deepEqual(markers.filter(marker => marker.kind === "item").map(marker => [marker.order, marker.item.id]), [[1, 3118], [2, 3157], [3, 3020]]);
  assert.deepEqual(spikeMarkers([]), []);
  assert.deepEqual(spikeMarkers([{ id: 0, name: "Empty" }]), []);
});
