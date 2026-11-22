import { test } from "node:test";
import assert from "node:assert/strict";
import { buildNameMaps, groupPages, mapPage, presetName, presetSelection, runePagesForAnswer } from "./proRunePages.ts";

// Minimal CommunityDragon fixtures with the real ids and style slots.
const PERKS = [
  { id: 8010, name: "Conqueror" }, { id: 8021, name: "Fleet Footwork" },
  { id: 9101, name: "Absorb Life" }, { id: 9111, name: "Triumph" }, { id: 8009, name: "Presence of Mind" },
  { id: 9104, name: "Legend: Alacrity" }, { id: 9105, name: "Legend: Haste" }, { id: 9103, name: "Legend: Bloodline" },
  { id: 8014, name: "Coup de Grace" }, { id: 8017, name: "Cut Down" }, { id: 8299, name: "Last Stand" },
  { id: 8306, name: "Hextech Flashtraption" }, { id: 8304, name: "Magical Footwear" }, { id: 8321, name: "Cash Back" },
  { id: 8313, name: "Triple Tonic" }, { id: 8352, name: "Time Warp Tonic" }, { id: 8345, name: "Biscuit Delivery" },
  { id: 8347, name: "Cosmic Insight" }, { id: 8410, name: "Approach Velocity" }, { id: 8316, name: "Jack of All Trades" },
  { id: 5008, name: "Adaptive Force" }, { id: 5005, name: "Attack Speed" }, { id: 5007, name: "Ability Haste" },
  { id: 5010, name: "Move Speed" }, { id: 5001, name: "Health Scaling" },
  { id: 5011, name: "Health" }, { id: 5013, name: "Tenacity and Slow Resist" },
];
const STYLES = {
  styles: [
    { id: 8000, name: "Precision", slots: [
      { type: "kKeyStone", perks: [8005, 8008, 8021, 8010] },
      { type: "kMixedRegularSplashable", perks: [9101, 9111, 8009] },
      { type: "kMixedRegularSplashable", perks: [9104, 9105, 9103] },
      { type: "kMixedRegularSplashable", perks: [8014, 8017, 8299] },
      { type: "kStatMod", perks: [5008, 5005, 5007] },
      { type: "kStatMod", perks: [5008, 5010, 5001] },
      { type: "kStatMod", perks: [5011, 5013, 5001] },
    ] },
    { id: 8300, name: "Inspiration", slots: [
      { type: "kKeyStone", perks: [8351, 8360, 8369] },
      { type: "kMixedRegularSplashable", perks: [8306, 8304, 8321] },
      { type: "kMixedRegularSplashable", perks: [8313, 8352, 8345] },
      { type: "kMixedRegularSplashable", perks: [8347, 8410, 8316] },
      { type: "kStatMod", perks: [5008, 5005, 5007] },
      { type: "kStatMod", perks: [5008, 5010, 5001] },
      { type: "kStatMod", perks: [5011, 5013, 5001] },
    ] },
  ],
};
const SPELLS = [
  { id: 2203, name: "Flash", gameModes: [] }, { id: 74, name: "Flash", gameModes: ["JADE"] },
  { id: 4, name: "Flash", gameModes: ["ARAM", "CLASSIC"] }, { id: 712, name: "Teleport", gameModes: ["JADE"] },
  { id: 21, name: "Barrier", gameModes: ["ARAM", "CLASSIC"] }, { id: 14, name: "Ignite", gameModes: ["CLASSIC"] },
  { id: 12, name: "Teleport", gameModes: ["CLASSIC", "SWIFTPLAY"] },
];
const maps = buildNameMaps(PERKS, STYLES, SPELLS);
const CORKI = ["Conqueror", "Presence of Mind", "Legend: Bloodline", "Cut Down", "Jack Of All Trades", "Cash Back", "Attack Speed", "Adaptive Force", "Health"];

test("name-to-id: verified Corki page maps runes, styles and spells", () => {
  const r = mapPage(CORKI, ["Flash", "Barrier"], maps);
  assert.equal(r.ok, true);
  assert.deepEqual(r.runes, [8010, 8009, 9103, 8017, 8316, 8321, 5005, 5008, 5011]);
  assert.equal(r.primaryStyle, 8000);
  assert.equal(r.subStyle, 8300);
  assert.deepEqual(r.spells, [4, 21]);
});

test("duplicate spell names resolve to the CLASSIC spell, not a mode variant", () => {
  assert.deepEqual(mapPage(CORKI, ["Teleport", "Flash"], maps).spells, [12, 4]);
});

test("shards resolve per row: same name, different rows; wrong row drops the page", () => {
  const flex = [...CORKI.slice(0, 6), "Adaptive Force", "Health Scaling", "Health Scaling"];
  assert.deepEqual(mapPage(flex, ["Flash"], maps).runes.slice(6), [5008, 5001, 5001]);
  const badRow = [...CORKI.slice(0, 6), "Health", "Adaptive Force", "Health"]; // Health is not a row-1 shard
  const r = mapPage(badRow, ["Flash"], maps);
  assert.equal(r.ok, false);
  assert.deepEqual(r.unmapped, ["Health"]);
});

test("unmapped names drop the whole page and are named", () => {
  const r = mapPage([...CORKI.slice(0, 3), "Martian Focus", ...CORKI.slice(4)], ["Flash", "Barrier"], maps);
  assert.equal(r.ok, false);
  assert.deepEqual(r.unmapped, ["Martian Focus"]);
  assert.equal(mapPage(CORKI.slice(0, 8), ["Flash"], maps).ok, false);
});

test("grouping: identical pages merge regardless of spell order; runner-up needs 3 games", () => {
  const page = (spells, win) => ({ runes: [8010, 8009, 9103, 8017, 8316, 8321, 5005, 5008, 5011], primaryStyle: 8000, subStyle: 8300, spells, win });
  const alt = (win) => ({ runes: [8021, 8009, 9103, 8017, 8316, 8321, 5005, 5008, 5011], primaryStyle: 8000, subStyle: 8300, spells: [4, 21], win });
  const pages = groupPages([page([4, 21], true), page([21, 4], false), page([4, 21], true), alt(true), alt(true)]);
  assert.equal(pages.length, 1); // alt has only 2 games
  assert.deepEqual([pages[0].games, pages[0].wins], [3, 2]);
  const withRunner = groupPages([page([4, 21], true), page([4, 21], true), page([4, 21], false), page([4, 21], true), alt(true), alt(false), alt(true)]);
  assert.equal(withRunner.length, 2);
  assert.deepEqual([withRunner[0].games, withRunner[1].games, withRunner[1].wins], [4, 3, 2]);
});

test("answer pages: first answer champion, missing vs dropped coverage", () => {
  const row = (runes, spells = "Flash,Barrier") => ({ Champion: "Corki", Role: "Bot", Runes: runes, SummonerSpells: spells, GameId: "g" });
  const rows = new Map([
    ["g1", [row(CORKI.join(","))]],
    ["g2", [row(CORKI.join(","))]],
    ["g3", [row("")]],
    ["g4", [row([...CORKI.slice(0, 3), "Martian Focus", ...CORKI.slice(4)].join(","))]],
  ]);
  const answer = { answer: ["Corki", "Alistar"], proGames: [
    { gameId: "g1", result: "Win" }, { gameId: "g2", result: "Loss" }, { gameId: "g3", result: "Win" }, { gameId: "g4", result: "Win" }, { gameId: "g5", result: "Win" },
  ] };
  const { pages, coverage } = runePagesForAnswer(answer, rows, maps);
  assert.deepEqual([coverage.total, coverage.withRunes, coverage.missing, coverage.dropped], [5, 2, 2, 1]);
  assert.deepEqual(coverage.unmapped, ["Martian Focus"]);
  assert.deepEqual([pages[0].games, pages[0].wins], [2, 1]);
});

test("preset selection slices the 9 ids; name stays within the 40-char vault cap", () => {
  const page = { runes: [8010, 8009, 9103, 8017, 8316, 8321, 5005, 5008, 5011], primaryStyle: 8000, subStyle: 8300, spells: [4, 21], games: 5, wins: 3 };
  assert.deepEqual(presetSelection(page), {
    primaryPageId: 8000, secondaryPageId: 8300, keystone: 8010,
    primaryRunes: [8009, 9103, 8017], secondaryRunes: [8316, 8321], shards: [5005, 5008, 5011],
  });
  assert.equal(presetName("Jhin"), "Pro vs Jhin");
  assert.ok(presetName("A".repeat(60)).length <= 40);
});
