import { test } from "node:test";
import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { GUIDE_SECTIONS, guideStep, itemChanges } from "./matchupGuide.ts";
import { ahriVsZed } from "./fixtures/ahriVsZed.ts";

test("guide contains only the four reading sections in order", () => {
  assert.deepEqual(GUIDE_SECTIONS, [["overview", "Overview"], ["early-lane", "Early lane"], ["mid-lane", "Mid lane"], ["win-condition", "Win condition"]]);
});

test("fixture uses four sections, separate itemization and never flags a live cooldown as outdated", () => {
  assert.deepEqual(Object.keys(ahriVsZed.sections), GUIDE_SECTIONS.map(([id]) => id));
  const tips = [...ahriVsZed.keyTips, ...ahriVsZed.itemization, ...Object.values(ahriVsZed.enemyTips).flat(),
    ...Object.values(ahriVsZed.sections).flatMap(section => [...section.tips, ...(section.alternatives ?? []).flatMap(style => style.tips)])];
  assert.ok(ahriVsZed.itemization.length);
  for (const source of ahriVsZed.sources) assert.doesNotMatch(source.rankClaim, /fixture|placeholder/i);
  for (const tip of tips.filter(tip => tip.outdated)) assert.doesNotMatch(tip.text, /cooldown|\{cd:/i);
  assert.ok(tips.some(tip => tip.text.includes("{cd:103:E}")));
  assert.ok(tips.some(tip => tip.text.includes("{cd:238:W}")));
  for (const tip of tips) {
    assert.doesNotMatch(tip.text, /\b\d+(?:\.\d+)?[- ]seconds?\b/);
    for (const id of tip.sourceIds ?? []) assert.ok(ahriVsZed.sources.some(source => source.id === id));
  }
});

test("guide fixture item references match the app's verified item catalog", () => {
  const { items } = JSON.parse(readFileSync(new URL("./fixtures/guideItems.json", import.meta.url), "utf8"));
  const catalog = new Map(items.map(item => [item.id, item]));
  const tips = [...ahriVsZed.keyTips, ...ahriVsZed.itemization, ...Object.values(ahriVsZed.enemyTips).flat(),
    ...Object.values(ahriVsZed.sections).flatMap(section => [...section.tips, ...(section.alternatives ?? []).flatMap(style => style.tips)])];
  for (const id of tips.flatMap(tip => tip.itemIds ?? [])) {
    assert.ok(catalog.get(id)?.name && catalog.get(id)?.iconPath, `Unknown item ${id}`);
  }
  assert.equal(catalog.get(2420).name, "Seeker's Armguard");
  assert.equal(catalog.has(3191), false);
});

test("guide navigation wraps, supports endpoints, and leaves unrelated keys alone", () => {
  assert.equal(guideStep(0, "ArrowLeft"), 3);
  assert.equal(guideStep(3, "ArrowRight"), 0);
  assert.equal(guideStep(1, "ArrowRight"), 2);
  assert.equal(guideStep(3, "Home"), 0);
  assert.equal(guideStep(3, "End"), 3);
  assert.equal(guideStep(3, "Escape"), null);
});

test("build comparison names reordered, new and removed items from data", () => {
  const usual = [{ id: 1, name: "Mana" }, { id: 2, name: "Damage" }, { id: 3, name: "Armor" }];
  const matchup = [{ id: 1, name: "Mana" }, { id: 3, name: "Armor" }, { id: 4, name: "Stasis" }];
  assert.deepEqual(itemChanges(usual, matchup), ["Armor: slot 3 → 2", "Stasis added in slot 3", "Damage omitted from matchup build"]);
  assert.deepEqual(itemChanges(usual, usual), []);
});
