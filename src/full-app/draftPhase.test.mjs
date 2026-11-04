import { test } from "node:test";
import assert from "node:assert/strict";
import { draftStage } from "./LiveGameModel.ts";

test("Draft switches between draft, loading/running game, and idle", () => {
  assert.equal(draftStage("ChampSelect", false), "draft");
  for (const phase of ["GameStart", "InProgress", "Reconnect"]) assert.equal(draftStage(phase, false), "live");
  for (const phase of ["", "None", "Lobby", "Matchmaking", "ReadyCheck", "WaitingForStats", "PreEndOfGame", "EndOfGame", "Offline"]) assert.equal(draftStage(phase, false), "idle");
  assert.equal(draftStage("ChampSelect", true), "live", "the live watcher wins over a stale rune view");
});
