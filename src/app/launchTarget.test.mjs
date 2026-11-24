import { test } from "node:test";
import { strict as assert } from "node:assert";
import { launchOverride, openLabel, resolveLaunchTarget } from "./launchTarget.ts";

test("the account override wins over the global setting", () => {
  assert.equal(openLabel("valorant", "league"), "Open VALORANT");
  assert.equal(openLabel("riot_client", "valorant"), "Open Riot Client");
});

test("no override follows the global setting, and League when that is unknown", () => {
  assert.equal(openLabel(null, "valorant"), "Open VALORANT");
  assert.equal(openLabel(undefined, "riot_client"), "Open Riot Client");
  assert.equal(openLabel(null, undefined), "Open League");
});

test("the picker shows the global choice for an account without an override", () => {
  assert.equal(resolveLaunchTarget(null, "valorant"), "valorant");
  assert.equal(resolveLaunchTarget("riot_client", "valorant"), "riot_client");
  assert.equal(resolveLaunchTarget(undefined, undefined), "league");
});

test("picking the global choice stores null, any other pick stores the override", () => {
  assert.equal(launchOverride("valorant", "valorant"), null);
  assert.equal(launchOverride("league", undefined), null);
  assert.equal(launchOverride("riot_client", "league"), "riot_client");
  assert.equal(launchOverride("league", "valorant"), "league");
});
