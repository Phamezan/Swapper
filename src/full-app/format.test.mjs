import { test } from "node:test";
import { strict as assert } from "node:assert";
import { fmtCompact, fmtCount } from "./ui/format.ts";

test("counts get en-US thousands separators", () => {
  assert.equal(fmtCount(183275), "183,275");
  assert.equal(fmtCount(42), "42");
  assert.equal(fmtCount(9.6), "10");
});

test("large stats use k and M with one decimal", () => {
  assert.equal(fmtCompact(950), "950");
  assert.equal(fmtCompact(1000), "1.0k");
  assert.equal(fmtCompact(22437), "22.4k");
  assert.equal(fmtCompact(26000), "26.0k");
  assert.equal(fmtCompact(999960), "1.0M");
  assert.equal(fmtCompact(1234567), "1.2M");
  assert.equal(fmtCompact(-1500), "-1.5k");
});
