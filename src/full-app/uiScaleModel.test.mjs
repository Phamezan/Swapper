import { test } from "node:test";
import { strict as assert } from "node:assert";
import { uiScale } from "./uiScaleModel.ts";

test("1080p and smaller windows keep zoom 1", () => {
  assert.equal(uiScale(1920, 1080), 1);
  assert.equal(uiScale(1100, 700), 1);
  assert.equal(uiScale(2560, 900), 1);
});

test("1440p zooms by the tighter axis", () => {
  assert.equal(uiScale(2560, 1440), 1.33);
  assert.equal(uiScale(2560, 1392), 1.29);
});

test("4K clamps at 1.5 and bad sizes fall back to 1", () => {
  assert.equal(uiScale(3840, 2160), 1.5);
  assert.equal(uiScale(0, 0), 1);
  assert.equal(uiScale(NaN, 1080), 1);
});
