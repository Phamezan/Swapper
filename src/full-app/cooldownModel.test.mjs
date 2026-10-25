import { test } from "node:test";
import { strict as assert } from "node:assert";
import { abilityDetails, perRank, tooltipParts } from "./cooldownModel.ts";

test("numeric tooltip formatting preserves text and distinguishes ranks and scaling", () => {
  const text = "Deals 75/150/225 (+60% AP) damage; below 9/10/11% (+0.5% per stack).";
  const parts = tooltipParts(text);
  assert.equal(parts.map(part => part.text).join(""), text);
  assert.deepEqual(parts.filter(part => part.kind).map(part => part.kind), ["ranks", "ap", "ranks", "stack"]);
  assert.equal(tooltipParts("Deals 20 (+50% bonus AD) and heals (+10% max Health).").filter(part => part.kind).map(part => part.kind).join(","), "ad,health");
  assert.deepEqual(tooltipParts("Short fallback description."), [{ text: "Short fallback description." }]);
});

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
