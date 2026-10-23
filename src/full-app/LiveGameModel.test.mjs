import { test } from "node:test";
import { strict as assert } from "node:assert";
import { crestSlug, enterLive, hasRecord, kda, leaveLive, pct, showInNav, roleOf, splitRiotId } from "./LiveGameModel.ts";

test("a game start switches to Live Game and remembers the page", () => {
  assert.deepEqual(enterLive("Matchups", null, null), { page: "Live Game", returnTo: "Matchups" });
  // Already there (window opened on Live Game): keep any earlier return page.
  assert.deepEqual(enterLive("Live Game", "Accounts", null), { page: "Live Game", returnTo: "Accounts" });
  assert.deepEqual(enterLive("Live Game", null, null), { page: "Live Game", returnTo: "Dashboard" });
});

test("a game right after champion select returns to the page before champion select", () => {
  assert.deepEqual(enterLive("Champion select", null, "Tier list & Builds"), { page: "Live Game", returnTo: "Tier list & Builds" });
  assert.deepEqual(enterLive("Champion select", null, null), { page: "Live Game", returnTo: "Dashboard" });
});

test("the game end restores the page only if the user is still on Live Game", () => {
  assert.equal(leaveLive("Live Game", "Matchups"), "Matchups");
  assert.equal(leaveLive("Live Game", null), "Dashboard");
  assert.equal(leaveLive("Settings", "Matchups"), null);
});

test("the sidebar lists Live Game only during a game", () => {
  const pages = ["Dashboard", "Live Game", "Matchups"];
  assert.deepEqual(pages.filter(p => showInNav(p, false)), ["Dashboard", "Matchups"]);
  assert.deepEqual(pages.filter(p => showInNav(p, true)), pages);
});

test("helpers", () => {
  assert.equal(pct(3, 4), 75);
  assert.equal(pct(0, 0), 0);
  assert.equal(kda({ kills: 4, deaths: 0, assists: 2 }), 6);
  assert.equal(crestSlug("SILVER"), "none");
  assert.equal(crestSlug("GRANDMASTER"), "master_plus");
  assert.equal(crestSlug("EMERALD"), "emerald_plus");
  assert.deepEqual(splitRiotId("A#B#EUW"), ["A#B", "#EUW"]);
  assert.equal(roleOf("UTILITY"), "support");
  assert.equal(roleOf(""), "");
});

test("a season with many wins and no recorded loss is the client's missing-losses gap", () => {
  const rank = (wins, losses) => ({ tier: "EMERALD", division: "II", lp: 48, wins, losses });
  assert.equal(hasRecord(rank(265, 0)), false);
  assert.equal(hasRecord(rank(103, 83)), true);
  assert.equal(hasRecord(rank(6, 0)), true);
});
