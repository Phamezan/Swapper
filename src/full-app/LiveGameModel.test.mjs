import { test } from "node:test";
import { strict as assert } from "node:assert";
import { crestSlug, draftRoute, enterLive, hasRecord, kda, leaveLive, pct, roleOf, splitRiotId } from "./LiveGameModel.ts";

test("a game start switches to Live Game and remembers the page", () => {
  assert.deepEqual(enterLive("Matchups", null, null), { page: "Champion select", returnTo: "Matchups" });
  // Already there (window opened on Live Game): keep any earlier return page.
  assert.deepEqual(enterLive("Champion select", "Accounts", null), { page: "Champion select", returnTo: "Accounts" });
  assert.deepEqual(enterLive("Live Game", null, null), { page: "Champion select", returnTo: "Dashboard" });
});

test("a game right after champion select returns to the page before champion select", () => {
  assert.deepEqual(enterLive("Champion select", null, "Tier list & Builds"), { page: "Champion select", returnTo: "Tier list & Builds" });
  assert.deepEqual(enterLive("Champion select", null, null), { page: "Champion select", returnTo: "Dashboard" });
});

test("the game end restores the page only if the user is still on Live Game", () => {
  assert.equal(leaveLive("Live Game", "Matchups"), "Matchups");
  assert.equal(leaveLive("Champion select", "Matchups"), "Matchups");
  assert.equal(leaveLive("Live Game", null), "Dashboard");
  assert.equal(leaveLive("Settings", "Matchups"), null);
});

test("old Live Game destinations redirect to Draft", () => {
  assert.equal(draftRoute("Live Game"), "Champion select");
  assert.equal(draftRoute("Champion select"), "Champion select");
  assert.equal(draftRoute("Matchups"), "Matchups");
});

test("helpers", () => {
  assert.equal(pct(3, 4), 75);
  assert.equal(pct(0, 0), 0);
  assert.equal(kda({ kills: 4, deaths: 0, assists: 2 }), 6);
  assert.deepEqual(splitRiotId("A#B#EUW"), ["A#B", "#EUW"]);
  assert.equal(roleOf("UTILITY"), "support");
  assert.equal(roleOf(""), "");
});

test("live ranks use the available tier-specific crests instead of neutral or shared fallbacks", () => {
  for (const tier of ["IRON", "BRONZE", "SILVER", "GOLD", "PLATINUM", "EMERALD", "DIAMOND", "MASTER", "GRANDMASTER", "CHALLENGER"]) {
    assert.equal(crestSlug(tier), tier.toLowerCase());
  }
  for (const tier of [null, undefined, "", "UNKNOWN"]) assert.equal(crestSlug(tier), "none");
});

test("a season with many wins and no recorded loss is the client's missing-losses gap", () => {
  const rank = (wins, losses) => ({ tier: "EMERALD", division: "II", lp: 48, wins, losses });
  assert.equal(hasRecord(rank(265, 0)), false);
  assert.equal(hasRecord(rank(103, 83)), true);
  assert.equal(hasRecord(rank(6, 0)), true);
});
