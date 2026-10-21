import { test } from "node:test";
import { strict as assert } from "node:assert";
import { sparkArea, sparkline } from "./sparkline.ts";

test("sparkline handles empty, single and flat samples without NaN", () => {
  assert.equal(sparkline([]), "");
  assert.equal(sparkline([5]), "50,14");
  assert.equal(sparkline([5,5]), "0,14 100,14");
});
test("sparkline preserves chronological order and scales within the viewbox", () => {
  assert.equal(sparkline([10,20,15]), "0,24 50,4 100,14");
});
test("sparkArea closes the line along the bottom edge", () => {
  assert.equal(sparkArea([]), "");
  assert.equal(sparkArea([10,20,15]), "M0,24 L50,4 L100,14 L100,28 L0,28 Z");
  assert.equal(sparkArea([5]), "M50,14 L50,28 L50,28 Z");
});
