import { test } from "node:test";
import { strict as assert } from "node:assert";
import { assignRoles, autoRows, mainRoles, mostPlayed } from "./championsModel.ts";

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

test("Auto uses LeBlanc's per-role games rather than list order or tier strength", () => {
  const roles = { top: [row(7, "S+", 55, 775)], mid: [row(7, "B", 50.5, 25689)] };
  assert.equal(mainRoles(roles).get(7), "mid");
  assert.deepEqual(autoRows(roles).map(r => [r.championId, r.role]), [[7, "mid"]]);
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

test("draft roles: Karthus with Shyvana is the bot laner, not a second jungler", () => {
  const lists = {
    top: [row(102, "C", 48, 3000)],
    jungle: [row(30, "A", 51, 30000), row(102, "S", 52, 60000)],
    mid: [row(30, "B", 49, 4000)],
    adc: [row(30, "B", 50, 8000)],
  };
  const team = (...ids) => ids.map(championId => ({ championId, role: "" }));
  assert.deepEqual(assignRoles(team(30, 102), lists), ["adc", "jungle"]);
  // Reported positions are kept; unpicked members and unknown champions fill what is left.
  assert.deepEqual(assignRoles([{ championId: 30, role: "jungle" }, { championId: 102, role: "" }], lists), ["jungle", "top"]);
  assert.deepEqual(new Set(assignRoles(team(30, 102, 0, 0, 999), lists)), new Set(["top", "jungle", "mid", "adc", "support"]));
  // Clashing positions or a sixth member cannot be placed.
  assert.deepEqual(assignRoles([{ championId: 30, role: "mid" }, { championId: 102, role: "mid" }], lists), []);
  assert.deepEqual(assignRoles(team(1, 2, 3, 4, 5, 6), lists), []);
});
