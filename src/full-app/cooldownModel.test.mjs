import { test } from "node:test";
import { strict as assert } from "node:assert";
import { abilityDetails, perRank } from "./cooldownModel.ts";

test("cooldowns read one value per rank, collapsing flat ones", () => {
  assert.equal(perRank([12, 11, 10, 9, 8]), "12 / 11 / 10 / 9 / 8");
  assert.equal(perRank([5, 4.5, 4, 3.5, 3]), "5 / 4.5 / 4 / 3.5 / 3");
  assert.equal(perRank([7, 7, 7, 7, 7]), "7");
  assert.equal(perRank([1 / 3]), "0.33");
});

test("nothing to show for empty or all-zero values", () => {
  assert.equal(perRank([]), "");
  assert.equal(perRank([0, 0, 0]), "");
});

test("details list charges, cost and range only when present", () => {
  const base = { key: "Q", name: "", iconPath: null, cooldowns: [7], costs: [], ranges: [], charges: 0, recharge: [] };
  assert.deepEqual(abilityDetails(base), []);
  assert.deepEqual(abilityDetails({ ...base, costs: [55, 65], ranges: [970, 970] }), ["Cost 55 / 65", "Range 970"]);
  assert.deepEqual(abilityDetails({ ...base, charges: 2, recharge: [24, 21.5] }), ["2 charges · 24 / 21.5 s recharge"]);
});
