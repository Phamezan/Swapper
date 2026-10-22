import { test } from "node:test";
import { strict as assert } from "node:assert";
import { changedKeys, clampStep } from "./onboardingModel.ts";

const base = { fullApp: false, startup: false, overlay: false, gold: true, objectives: true, skills: true, deceive: false, koreanFont: false, remote: false };

test("only choices that differ from the saved state are written", () => {
  assert.deepEqual(changedKeys(base, base), []);
  assert.deepEqual(changedKeys(base, { ...base, fullApp: true, remote: true }), ["fullApp", "remote"]);
});

test("steps stay inside the three-step range", () => {
  assert.equal(clampStep(-1), 0);
  assert.equal(clampStep(1), 1);
  assert.equal(clampStep(3), 2);
});
