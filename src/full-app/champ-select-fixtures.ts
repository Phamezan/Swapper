// Anonymised champion select states for the model tests and the dev-only
// preview (champ-select-preview.html). No names, ids or PUUIDs of real players.
import type { RunesView } from "../runes/types";
import type { Draft } from "./champ-select-model";

export type FixtureName = "planning" | "hover" | "locked" | "locked-blind";

const member = (championId: number, position: string, locked: boolean, local = false) => ({ championId, position, local, locked });
const selection = { primaryPageId: 8000, secondaryPageId: 8300, keystone: 8010, primaryRunes: [9111, 9104, 8299], secondaryRunes: [8304, 8347], shards: [5005, 5008, 5001] };

export function fixtureDraft(name: FixtureName): Draft {
  const own = name === "planning" ? 0 : 266;
  const enemyMid = name === "locked-blind" ? 0 : 103;
  return {
    allies: [member(64, "jungle", true), member(own, "top", name.startsWith("locked"), true), member(0, "middle", false), member(222, "bottom", true), member(0, "utility", false)],
    enemies: [member(122, "top", true), member(0, "jungle", false), member(enemyMid, "middle", enemyMid > 0), member(0, "bottom", false), member(0, "utility", false)]
      .map(enemy => name === "locked-blind" && enemy.position === "top" ? { ...enemy, championId: 0, locked: false } : enemy),
    bans: [157, 777, 893, 875, 233, 902],
    timer: { phase: name.startsWith("locked") ? "FINALIZATION" : "BAN_PICK", remainingMs: name.startsWith("locked") ? 41_000 : 24_000 },
  };
}

export function fixtureView(name: FixtureName): RunesView {
  const locked = name.startsWith("locked");
  const own = name === "planning" ? 0 : 266;
  const enemy = name === "locked-blind" ? 0 : 122;
  return {
    phase: "ChampSelect", championId: own, championName: own ? "Aatrox" : "", position: "top", mode: "ranked",
    source: "lolalytics", sourceLabel: "lolalytics", message: null,
    presets: own ? [{ index: 0, title: "Conqueror", ...selection, winPct: 51.2, play: 41000, spells: [4, 12] }] : [],
    trees: [], shards: [],
    applied: locked ? { name: "Swapper: Aatrox", championId: own, ...selection, presetIndex: 0 } : null,
    autoApply: false, importItems: true, canApply: locked, locked,
    spells: { spell1Id: 4, spell2Id: 14, available: [{ id: 4, name: "Flash" }, { id: 12, name: "Teleport" }, { id: 14, name: "Ignite" }], applyWithRunes: true },
    tier: "diamond_plus", tierLabel: "Diamond+", tiers: [], tierSupported: true, tierEmpty: false,
    enemyChampionId: enemy, enemyChampionName: enemy ? "Darius" : "", enemies: enemy ? [{ id: enemy, name: "Darius" }] : [], enemySlots: 5,
    games: 41000, stale: false, updatedAt: null,
  } as RunesView;
}
