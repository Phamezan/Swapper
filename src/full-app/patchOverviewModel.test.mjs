import { test } from "node:test";
import { strict as assert } from "node:assert";
import { entryAnchor, patchOverview } from "./patchOverviewModel.ts";
import { sectionId } from "./PatchNotesModel.ts";

const line = (label, before, after) => ({ label, before, after, text: null });
const champion = (name, ...lines) => ({ name, context: null, groups: [{ title: "Q", lines }], notes: [] });

test("the overview groups champions by verdict and links every card", () => {
  const sections = [
    { title: "Champions", paragraphs: [], entries: [
      champion("Lee Sin", line("Kick Damage", "175 / 400 / 625", "150 / 350 / 550")),
      champion("Azir", line("Damage", "60 / 80", "70 / 90")),
      champion("Taric", line("Damage", "60 / 80", "50 / 90")),
      { name: "", context: null, groups: [], notes: [] },
    ] },
    { title: "Items", paragraphs: [], entries: [{ name: "Hubris", itemId: 6697, context: null, groups: [], notes: [] }] },
    { title: "Systems", paragraphs: ["Jungle changes"], entries: [] },
    { title: "Bugfixes", paragraphs: ["Fixed a bug"], entries: [] },
  ];
  const overview = patchOverview(sections, [{ id: 64, name: "Lee Sin" }, { id: 268, name: "Azir" }]);
  assert.deepEqual(overview.nerfs.map(c => [c.name, c.championId, c.anchor]), [["Lee Sin", 64, entryAnchor("Champions", 0)]]);
  assert.deepEqual(overview.buffs.map(c => c.name), ["Azir"]);
  assert.deepEqual(overview.adjusted.map(c => [c.name, c.championId]), [["Taric", null]]);
  assert.deepEqual(overview.items, [{ anchor: entryAnchor("Items", 0), name: "Hubris", itemId: 6697 }]);
  // Systems link to their section; bug fixes are not part of the overview.
  assert.deepEqual(overview.systems, [{ anchor: sectionId("Systems"), name: "Systems" }]);
});

test("an empty patch has an empty overview", () => {
  assert.deepEqual(patchOverview([], []), { nerfs: [], buffs: [], adjusted: [], items: [], systems: [] });
});
