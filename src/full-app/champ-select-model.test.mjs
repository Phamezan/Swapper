import { test } from "node:test";
import { strict as assert } from "node:assert";
import { applyStatus, counterPicks, defaultTab, enemyLaner, maxOrder, metaPicks, pageMode, recommendedSuggestions, secondsLeft, skillGrid, takenIds } from "./champ-select-model.ts";
import { fixtureDraft, fixtureView } from "./champ-select-fixtures.ts";

test("the page plans until the pick is locked and builds after", () => {
  assert.equal(pageMode(null), "idle");
  assert.equal(pageMode({ ...fixtureView("planning"), phase: "InProgress" }), "idle");
  assert.equal(pageMode(fixtureView("planning")), "planning");
  assert.equal(pageMode(fixtureView("hover")), "planning");
  assert.equal(pageMode(fixtureView("locked")), "build");
  // Un-hovering or swapping before lock-in returns to planning.
  assert.equal(pageMode({ ...fixtureView("locked"), locked: false }), "planning");
  assert.equal(pageMode({ ...fixtureView("locked"), championId: 0 }), "planning");
  assert.equal(pageMode({ ...fixtureView("hover"), mode: "aram" }), "build");
});

test("suggestions default to counters only when the enemy laner is known", () => {
  assert.equal(defaultTab(122), "counters");
  assert.equal(defaultTab(0), "recommended");
});

test("the enemy laner is the revealed enemy in your role, or the only one in blind pick", () => {
  assert.equal(enemyLaner(fixtureDraft("locked"), "top"), 122);
  assert.equal(enemyLaner(fixtureDraft("locked"), "mid"), 103);
  assert.equal(enemyLaner(fixtureDraft("locked"), "jungle"), 0);
  assert.equal(enemyLaner(fixtureDraft("locked-blind"), "top"), 0);
  assert.equal(enemyLaner(null, "top"), 0);
  const blind = { allies: [], bans: [], timer: null, enemies: [{ championId: 7, position: "", local: false, locked: true }] };
  assert.equal(enemyLaner(blind, ""), 7);
  assert.equal(enemyLaner({ ...blind, enemies: [...blind.enemies, { championId: 8, position: "", local: false, locked: true }] }, ""), 0);
});

test("bans and other locked picks cannot be suggested, your own hover can", () => {
  const taken = takenIds(fixtureDraft("hover"));
  for (const id of [157, 64, 222, 122, 103]) assert.ok(taken.has(id), String(id));
  assert.ok(!taken.has(266));
});

test("counters are the enemy laner's worst reliable matchups, as your win rate", () => {
  const row = (championId, winPct, games, lowSample = false) => ({ championId, name: String(championId), winPct, delta: 0, games, lowSample });
  const picks = counterPicks([row(1, 47, 900), row(2, 44, 50, true), row(3, 45, 800), row(4, 53, 900), row(5, 46, 700)], new Set([5]), 2);
  assert.deepEqual(picks.map(p => [p.championId, p.winPct]), [[3, 55], [1, 53]]);
});

test("meta picks follow tier then win rate and skip taken champions", () => {
  const row = (championId, tier, winPct) => ({ championId, name: String(championId), tier, winPct, pickPct: 1, banPct: 1, games: 1000 });
  assert.deepEqual(metaPicks([row(1, "A", 54), row(2, "S", 50), row(3, "S+", 49), row(4, "S", 52), row(5, "S+", 53)], new Set([5])).map(p => p.championId), [3, 4, 2, 1]);
});

test("recommended picks keep their order and name unknown champions plainly", () => {
  const picks = [{ championId: 266, winRate: 51.5, games: 9000, flex: false }, { championId: 157, winRate: 50, games: 8000, flex: true }, { championId: 1, winRate: 50, games: 100, flex: true }];
  const shown = recommendedSuggestions(picks, new Map([[266, "Aatrox"]]), new Set([157]));
  assert.deepEqual(shown.map(p => [p.name, p.flex]), [["Aatrox", false], ["Champion 1", true]]);
});

test("apply status compares the shown build with what the client has", () => {
  const view = fixtureView("locked");
  const page = { ...view.applied };
  const status = applyStatus(view, 266, page, [4, 14], "266:1,2", "266:1,2");
  assert.deepEqual(status, { runes: true, spells: true, items: true });
  // Switching build tab: another page, other spells and another item set are not applied.
  assert.deepEqual(applyStatus(view, 266, { ...page, keystone: 8005 }, [4, 12], "266:3", "266:1,2"), { runes: false, spells: false, items: false });
  // Rune order inside a tree does not matter; shard rows do.
  assert.equal(applyStatus(view, 266, { ...page, primaryRunes: [...page.primaryRunes].reverse() }, [], "", null).runes, true);
  assert.equal(applyStatus(view, 266, { ...page, shards: [...page.shards].reverse() }, [], "", null).runes, false);
  // Another champion's page never counts, and nothing counts outside champion select.
  assert.equal(applyStatus(view, 103, page, [4, 14], "", null).runes, false);
  assert.deepEqual(applyStatus({ ...view, phase: "InProgress" }, 266, page, [4, 14], "", null), { runes: false, spells: false, items: false });
  assert.equal(applyStatus(view, 266, page, [], "", "").items, false);
});

test("the skill grid spans levels 1-18 and leaves unreported levels empty", () => {
  const grid = skillGrid("QEWQ?R");
  assert.equal(grid.length, 4);
  assert.ok(grid.every(row => row.cells.length === 18));
  assert.deepEqual(grid[0].cells.slice(0, 6), [1, null, null, 4, null, null]);
  assert.deepEqual(grid[3].cells.slice(0, 6), [null, null, null, null, null, 6]);
  assert.ok(grid.every(row => row.cells.slice(6).every(cell => cell === null)));
  assert.deepEqual(skillGrid(["Q", "W"])[1].cells.slice(0, 2), [null, 2]);
  assert.ok(skillGrid(null).every(row => row.cells.every(cell => cell === null)));
});

test("the max order uses the reported priority, else the order skills fill up in", () => {
  assert.deepEqual(maxOrder("QWEQQRQEQEREEWW", "qew"), ["Q", "E", "W"]);
  assert.deepEqual(maxOrder("QWEQQRQEQEREEWW"), ["Q", "E", "W"]);
  assert.deepEqual(maxOrder("EQWEERWEWRWEQQQ"), ["E", "W", "Q"]);
  assert.deepEqual(maxOrder("QWE", null), []);
  assert.deepEqual(maxOrder(null, "QQW"), []);
});

test("the timer counts down from when the draft was read", () => {
  assert.equal(secondsLeft({ phase: "BAN_PICK", remainingMs: 24000 }, 1000, 1000), 24);
  assert.equal(secondsLeft({ phase: "BAN_PICK", remainingMs: 24000 }, 1000, 11500), 14);
  assert.equal(secondsLeft({ phase: "BAN_PICK", remainingMs: 2000 }, 0, 9000), 0);
  assert.equal(secondsLeft(null, 0, 0), null);
});

test("champion search ignores case and punctuation, prefers prefix matches and skips taken", async () => {
  const { searchChampions } = await import("./champ-select-model.ts");
  const names = new Map([[1, "Kai'Sa"], [2, "Kayle"], [3, "Ahri"], [4, "Rek'Sai"], [5, "Kayn"]]);
  assert.deepEqual(searchChampions(names, "kai", new Set()).map(c => c.name), ["Kai'Sa"]);
  assert.deepEqual(searchChampions(names, "sa", new Set()).map(c => c.name), ["Kai'Sa", "Rek'Sai"]);
  assert.deepEqual(searchChampions(names, "ka", new Set([5])).map(c => c.name), ["Kai'Sa", "Kayle"]);
  assert.deepEqual(searchChampions(names, "  ", new Set()), []);
});
