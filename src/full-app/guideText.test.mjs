import { test } from "node:test";
import assert from "node:assert/strict";
import { parseGuideText, liveCooldown, guideReferences } from "./guideText.ts";

test("items and runes share longest-first matching with abilities and cooldowns", () => {
  const ability = { kind: "ability", champion: 89, key: "W", name: "Eclipse", icon: null };
  const item = { kind: "item", id: 6692, name: "Eclipse" };
  const blade = { kind: "item", id: 3071, name: "Black Cleaver" };
  const rune = { kind: "rune", id: 8010, name: "Conqueror" };
  const short = { kind: "ability", champion: 1, key: "Q", name: "Cleaver", icon: null };
  const refs = guideReferences([ability, short], [item, blade], [rune]);
  assert.deepEqual(parseGuideText("Eclipse, Black Cleaver and Conqueror. {cd:89:W}", refs), [
    ability, { text: ", " }, blade, { text: " and " }, rune, { text: ". " }, { champion: 89, key: "W" },
  ]);
  assert.deepEqual(parseGuideText("Eclipse", [item, ability]), [ability]);
  assert.deepEqual(parseGuideText("Eclipse Conquerors conqueror éConqueror", [rune]), [
    { text: "Eclipse Conquerors conqueror éConqueror" },
  ]); // Unlisted items remain plain text.
  assert.deepEqual(parseGuideText("Conqueror"), [{ text: "Conqueror" }]);
});

test("ambiguous runes and rune names colliding with tip abilities or items are skipped", () => {
  const ability = { kind: "ability", champion: 1, key: "Q", name: "Aftershock", icon: null };
  const item = { kind: "item", id: 1, name: "Conqueror" };
  assert.deepEqual(guideReferences([ability], [item], [
    { kind: "rune", id: 2, name: "Aftershock" }, { kind: "rune", id: 3, name: "Conqueror" },
    { kind: "rune", id: 4, name: "Guardian" }, { kind: "rune", id: 5, name: "Resolve" },
  ]), [ability, item, { kind: "rune", id: 4, name: "Guardian" }]);
});

test("ability names match longest whole phrases without changing cooldown tokens", () => {
  const short = { kind: "ability", champion: 1, key: "Q", name: "Trap", icon: null };
  const long = { kind: "ability", champion: 240, key: "Q", name: "Bear Trap on a Rope", icon: "q.png" };
  const punctuated = { kind: "ability", champion: 240, key: "R", name: "Chaaaaaaaarge!!!", icon: null };
  assert.deepEqual(parseGuideText("Dodge Bear Trap on a Rope; Trap, Trapper, trap, éTrap. {cd:240:Q} Chaaaaaaaarge!!!", [short, long, punctuated]), [
    { text: "Dodge " }, long, { text: "; " }, short,
    { text: ", Trapper, trap, éTrap. " }, { champion: 240, key: "Q" }, { text: " " }, punctuated,
  ]);
  assert.deepEqual(parseGuideText("Dodge Bear Trap on a Rope"), [{ text: "Dodge Bear Trap on a Rope" }]);
});

test("cooldown tokens preserve prose and identify each champion and ability", () => {
  assert.deepEqual(parseGuideText("W: {cd:238:W}; E: {cd:103:E}."), [
    { text: "W: " }, { champion: 238, key: "W" }, { text: "; E: " }, { champion: 103, key: "E" }, { text: "." },
  ]);
  assert.deepEqual(parseGuideText("{cd:238:W}{cd:103:E}"), [{ champion: 238, key: "W" }, { champion: 103, key: "E" }]);
  assert.deepEqual(parseGuideText("{cd:0:Q} {cd:238:P} {cd:zed:W} {cd:238:W"), [{ text: "{cd:0:Q} {cd:238:P} {cd:zed:W} {cd:238:W" }]);
});

test("cooldown labels follow live per-rank values, including charge recharge", () => {
  const ability = { name: "Living Shadow", cooldowns: [20, 18.5, 17, 15.5, 14], charges: 0, recharge: [] };
  assert.equal(liveCooldown(ability, "W"), "20/18.5/17/15.5/14s");
  assert.equal(liveCooldown({ ...ability, cooldowns: [18, 17, 16, 15, 14] }, "W"), "18/17/16/15/14s");
  assert.equal(liveCooldown({ ...ability, cooldowns: [12, 12, 12] }, "W"), "12s");
  assert.equal(liveCooldown({ ...ability, charges: 2, recharge: [9, 8, 7] }, "W"), "9/8/7s");
});

test("unavailable or invalid cooldowns show only the ability name", () => {
  assert.equal(liveCooldown(undefined, "Living Shadow"), "Living Shadow");
  for (const cooldowns of [[], [0, 0], [NaN], [Infinity], [-1]]) {
    assert.equal(liveCooldown({ name: "Charm", cooldowns, charges: 0, recharge: [] }, "E"), "Charm");
  }
});


test("capitalized utility runes match while all five tree names stay plain", () => {
  const names = ["Triumph", "Revitalize", "Conditioning", "Overgrowth", "Unflinching", "Demolish", "Guardian"];
  const runes = names.map((name, id) => ({ kind: "rune", id, name }));
  const trees = ["Precision", "Domination", "Resolve", "Inspiration", "Sorcery"].map((name, id) => ({ kind: "rune", id: id + 100, name }));
  assert.deepEqual(guideReferences([], [], [...runes, ...trees]), runes);
  for (const rune of runes) {
    assert.deepEqual(parseGuideText(rune.name, runes), [rune]);
    assert.deepEqual(parseGuideText(rune.name.toLowerCase(), runes), [{ text: rune.name.toLowerCase() }]);
  }
});

test("catalog prefixes also match capitalized bare aliases and preserve whole words", () => {
  for (const name of ["Legend: Alacrity", "Legend: Haste", "Legend: Bloodline", "Future: Focus"] ) {
    const rune = { kind: "rune", id: 9104, name };
    const alias = name.split(": ")[1];
    const refs = guideReferences([], [], [rune]);
    assert.deepEqual(parseGuideText(name, refs), [rune]);
    assert.deepEqual(parseGuideText(alias, refs), [{ ...rune, name: alias }]);
    for (const text of [alias.toLowerCase(), `ability ${alias.toLowerCase()}`, `${alias}ful`, `é${alias}`]) {
      assert.deepEqual(parseGuideText(text, refs), [{ text }]);
    }
    const item = { kind: "item", id: 1, name: alias };
    assert.deepEqual(guideReferences([], [item], [rune]), [item]);
  }
});
