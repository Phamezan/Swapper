import { test } from "node:test";
import { strict as assert } from "node:assert";
import { sparkline } from "./sparkline.ts";

test("sparkline handles empty, single and flat samples without NaN", () => {
  assert.equal(sparkline([]), "");
  assert.equal(sparkline([5]), "50,14");
  assert.equal(sparkline([5,5]), "0,14 100,14");
});
test("sparkline preserves chronological order and scales within the viewbox", () => {
  assert.equal(sparkline([10,20,15]), "0,24 50,4 100,14");
});
