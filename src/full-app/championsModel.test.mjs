import { test } from "node:test";
import { strict as assert } from "node:assert";
import { autoRows, mainRoles, mostPlayed } from "./championsModel.ts";

const row = (championId, tier, winPct, games) => ({ championId, name: `C${championId}`, tier, winPct, games, pickPct: 1, banPct: 1 });
const lists = {
  // Aatrox (266) is played top far more than jungle.
  top: [row(266, "S", 51, 90000), row(1, "B", 49, 5000)],
  jungle: [row(266, "C", 48, 4000), row(64, "S+", 52, 80000)],
  mid: [row(103, "A", 51, 70000)],
};

test("a champion's main role is the role with the most games", () => {
  const roles = mainRoles(lists);
  assert.equal(roles.get(266), "top");
  assert.equal(roles.get(64), "jungle");
  assert.equal(roles.get(1), "top");
});

test("auto rows list each champion once at its main role, strongest tier first", () => {
  const rows = autoRows(lists);
  assert.deepEqual(rows.map(r => [r.championId, r.role]), [[64, "jungle"], [266, "top"], [103, "mid"], [1, "top"]]);
});

test("mostPlayed ranks champions by games and keeps each one's main role", () => {
  const games = [266, 266, 266, 64, 64, 103].map((championId, i) => ({ championId, role: i === 2 ? "jungle" : championId === 64 ? "jungle" : "top" }));
  assert.deepEqual(mostPlayed(games, 2), [{ championId: 266, role: "top", games: 3 }, { championId: 64, role: "jungle", games: 2 }]);
});

test("mostPlayed falls back to auto when the role is unknown", () => {
  assert.equal(mostPlayed([{ championId: 1, role: "" }])[0].role, "auto");
});

test('the champion pool puts recent picks first, then mastery by last played, without duplicates', async () => {
  const { championPool } = await import('./championsModel.ts');
  const recent = [{ championId: 1, role: 'top' }];
  const mastery = [{ championId: 2, lastPlayed: 5 }, { championId: 1, lastPlayed: 9 }, { championId: 3, lastPlayed: 7 }];
  assert.deepEqual(championPool(recent, mastery), [{ championId: 1, role: 'top' }, { championId: 3, role: 'auto' }, { championId: 2, role: 'auto' }]);
  assert.equal(championPool(recent, mastery, 2).length, 2);
});
