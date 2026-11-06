import { test } from "node:test";
import { strict as assert } from "node:assert";
import { displayPatch } from "./patchDisplay.ts";

for (const [input, expected] of [
  ["15.24", "25.24"], ["16.1", "26.1"], ["16.22", "26.22"],
  ["16.22.1", "26.22"], ["26.22", "26.22"], ["garbage", "garbage"],
  ["14.24", "14.24"], ["19.1", "29.1"], ["20.1", "20.1"],
  ["16.22.bad", "16.22.bad"],
]) test(`display patch ${input}`, () => assert.equal(displayPatch(input), expected));
