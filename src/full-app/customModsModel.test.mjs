import { test } from "node:test";
import assert from "node:assert/strict";
import { isRegionBlocked, changeNotice, nextNotice, modMeta, browseArgs, visibleMods, installedMod, isInstalled, categoryLabel, notWorking, formatModDate, modInitial, clashIds, libraryCounts, libraryOptions, libraryCategoryLabel, filterLibrary, modHealth, needsEnableConfirm } from "./customModsModel.ts";

const mod = (id, extra = {}) => ({
  id, name: id, author: "Ada", version: "1.0.0", category: null, champion: null,
  slotLabel: "Other", enabled: true, ...extra,
});

test("korea and china accounts are blocked, others are not", () => {
  assert.equal(isRegionBlocked("KR", "KR"), true);
  assert.equal(isRegionBlocked("CN1", null), true);
  assert.equal(isRegionBlocked(null, "China"), true);
  assert.equal(isRegionBlocked("EUW1", "EUW"), false);
  assert.equal(isRegionBlocked(null, null), false);
});

test("a waiting change says what it waits for", () => {
  const view = { mods: [], regionBlocked: false, inGame: false, appliesNextGame: false, needsClose: false };
  assert.equal(changeNotice(view), null);
  assert.equal(changeNotice({ ...view, needsClose: true }), "Close LTK Manager to apply this.");
  assert.equal(changeNotice({ ...view, inGame: true, appliesNextGame: true, needsClose: true }), "Applies next game.");
  assert.equal(nextNotice("Close LTK Manager to apply this.", view), null, "the wait is over");
  assert.equal(nextNotice("Another mod in the same slot was turned off (A).", view), "Another mod in the same slot was turned off (A).");
});

const rf = (id, extra = {}) => ({
  id, slug: id, name: id, author: "Ada", category: "champion_skin", champions: ["Ahri"],
  status: "working", thumbnailUrl: null, downloadCount: 5, viewCount: 9, likeCount: 0, publishedAt: null, updatedAt: null, licence: null,
  pageUrl: `https://runeforge.dev/mods/${id}`, tags: [], ...extra,
});

test("browse args build the runeforge_list query", () => {
  assert.deepEqual(
    browseArgs({ page: 2, search: "  ahri ", category: "champion_skin", championId: 103, sort: "downloads" }),
    { page: 2, search: "ahri", category: "champion_skin", championId: 103, sort: "downloads" },
  );
  assert.deepEqual(
    browseArgs({ page: 0, search: "", category: "", championId: null, sort: "trending" }),
    { page: 1, search: "", category: null, championId: null, sort: "trending" },
  );
});

test("working-only filter keeps working mods", () => {
  const mods = [rf("a"), rf("b", { status: "broken" }), rf("c", { status: null })];
  assert.deepEqual(visibleMods(mods, true).map((m) => m.id), ["a"]);
  assert.equal(visibleMods(mods, false).length, 3);
});

test("installed matching uses the runeforge mod uuid", () => {
  const library = [mod("lib1", { runeforgeModId: "rf-1" }), mod("lib2")];
  assert.equal(installedMod(rf("rf-1"), library)?.id, "lib1");
  assert.equal(isInstalled(rf("rf-1"), library), true);
  assert.equal(isInstalled(rf("rf-2"), library), false);
});

test("category labels, patch status, dates and initials", () => {
  assert.equal(categoryLabel("loading_screen"), "Loading screens");
  assert.equal(categoryLabel(null), "Other");
  assert.equal(categoryLabel("custom_thing"), "custom_thing");
  assert.equal(notWorking(rf("x")), false);
  assert.equal(notWorking(rf("x", { status: "Broken" })), true);
  assert.equal(notWorking(rf("x", { status: null })), false);
  assert.equal(formatModDate("2026-11-11T23:30:00.000+00:00"), "Nov 11, 2026");
  assert.equal(formatModDate(null), "");
  assert.equal(formatModDate("soon"), "");
  assert.equal(modInitial("  ahri skin"), "A");
  assert.equal(modInitial("[HUD] clean"), "H");
  assert.equal(modInitial("   "), "?");
});

test("row meta joins slot, version and licence", () => {
  assert.equal(modMeta(mod("x", { slotLabel: "Skin · Ahri", licence: "MIT" })), "Skin · Ahri · v1.0.0 · MIT");
  assert.equal(modMeta(mod("x", { version: "" })), "Other");
});

const library = [
  mod("a", { name: "Star Guardian Ahri", author: "Ada", category: "champion-skin", champion: "Ahri", slotLabel: "Skin · Ahri" }),
  mod("b", { name: "arcade ahri", author: "Bo", category: "champion-skin", champion: "Ahri", slotLabel: "Skin · Ahri", enabled: false }),
  mod("c", { name: "Clean HUD", author: "Cy", category: "hud", slotLabel: "HUD" }),
  mod("d", { name: "Big Sounds", author: "Ada", category: null, slotLabel: "Other", enabled: false }),
  mod("e", { name: "More Sounds", author: "Di", category: null, slotLabel: "Other", health: "repairable", fatals: 1411, errors: 0 }),
];
const all = { search: "", category: "", champion: "", tab: "all" };

test("clashes are mods sharing an exclusive slot, never Other", () => {
  assert.deepEqual([...clashIds(library)].sort(), ["a", "b"]);
  assert.deepEqual(libraryCounts(library), { all: 5, on: 3, off: 2, clashes: 2, problems: 1 });
  assert.deepEqual(libraryCounts([]), { all: 0, on: 0, off: 0, clashes: 0, problems: 0 });
});

test("library filters by search, category, champion and tab, sorted by name", () => {
  assert.deepEqual(filterLibrary(library, all).map((m) => m.id), ["b", "d", "c", "e", "a"]);
  assert.deepEqual(filterLibrary(library, { ...all, search: " AHRI " }).map((m) => m.id), ["b", "a"]);
  assert.deepEqual(filterLibrary(library, { ...all, search: "ada" }).map((m) => m.id), ["d", "a"], "search matches the author");
  assert.deepEqual(filterLibrary(library, { ...all, category: "hud" }).map((m) => m.id), ["c"]);
  assert.deepEqual(filterLibrary(library, { ...all, champion: "Ahri", tab: "on" }).map((m) => m.id), ["a"]);
  assert.deepEqual(filterLibrary(library, { ...all, tab: "off" }).map((m) => m.id), ["b", "d"]);
  assert.deepEqual(filterLibrary(library, { ...all, tab: "clashes" }).map((m) => m.id), ["b", "a"]);
  assert.deepEqual(filterLibrary(library, { ...all, tab: "problems" }).map((m) => m.id), ["e"]);
  assert.deepEqual(filterLibrary(library, { ...all, search: "zzz" }), []);
});

test("library filter options and tag labels", () => {
  assert.deepEqual(libraryOptions(library), { categories: ["champion-skin", "hud"], champions: ["Ahri"] });
  assert.equal(libraryCategoryLabel("champion-skin"), "Skins");
  assert.equal(libraryCategoryLabel("custom"), "custom");
});

test("LTK health: fatals or unrepairable is broken, repairable can be repaired, unknown is fine", () => {
  assert.deepEqual(modHealth(mod("x", { health: "repairable", fatals: 1411 })), { broken: true, repairable: true });
  assert.deepEqual(modHealth(mod("x", { health: "repairable", fatals: 0 })), { broken: false, repairable: true });
  assert.deepEqual(modHealth(mod("x", { health: "unrepairable", fatals: 0 })), { broken: true, repairable: false });
  assert.equal(modHealth(mod("x", { health: "healthy", fatals: 0 })), null);
  assert.equal(modHealth(mod("x")), null, "no verdict yet");
  assert.equal(needsEnableConfirm(mod("x", { health: "repairable", fatals: 3 })), true);
  assert.equal(needsEnableConfirm(mod("x", { health: "unrepairable", fatals: 0 })), false);
});
