import { test } from "node:test";
import { strict as assert } from "node:assert";
import { autoRows, mainRoles } from "./championsModel.ts";

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
