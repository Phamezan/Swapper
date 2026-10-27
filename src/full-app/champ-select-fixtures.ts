// Anonymised champion select states for the model tests and the dev-only
// preview (champ-select-preview.html). No names, ids or PUUIDs of real players.
import type { RunesView } from "../runes/types";
import type { Draft } from "./champ-select-model";

/** hidden-roles: ranked hides enemy positions, so two revealed enemies could be your laner. */
export type FixtureName = "planning" | "hover" | "locked" | "locked-blind" | "hidden-roles" | "locked-guide";

const member = (championId: number, position: string, locked: boolean, local = false) => ({ championId, position, local, locked });
const selection = { primaryPageId: 8000, secondaryPageId: 8300, keystone: 8010, primaryRunes: [9111, 9104, 8299], secondaryRunes: [8304, 8347], shards: [5005, 5008, 5001] };

export function fixtureDraft(name: FixtureName): Draft {
  const guide = name === "locked-guide";
  const own = guide ? 103 : name === "planning" ? 0 : 266;
  const enemyMid = guide ? 238 : name === "locked-blind" ? 0 : 103;
  return {
    allies: [member(64, "jungle", true), member(guide ? 266 : own, "top", name.startsWith("locked"), !guide), member(guide ? own : 0, "middle", guide, guide), member(222, "bottom", true), member(0, "utility", false)],
    enemies: [member(122, "top", true), member(0, "jungle", false), member(enemyMid, "middle", enemyMid > 0), member(0, "bottom", false), member(0, "utility", false)]
      .map(enemy => name === "locked-blind" && enemy.position === "top" ? { ...enemy, championId: 0, locked: false } : enemy)
      .map((enemy, i) => name === "hidden-roles" ? { ...enemy, position: "", championId: [122, 0, 24, 0, 0][i], locked: false } : enemy),
    bans: [157, 777, 893, 875, 233, 902],
    allyBans: [157, 893, 233], enemyBans: [777, 875, 902], canLock: !name.startsWith("locked"),
    timer: { phase: name.startsWith("locked") ? "FINALIZATION" : "BAN_PICK", remainingMs: name.startsWith("locked") ? 41_000 : 24_000 },
  };
}

export function fixtureView(name: FixtureName): RunesView {
  const guide = name === "locked-guide";
  const locked = name.startsWith("locked");
  const own = guide ? 103 : name === "planning" ? 0 : 266;
  const enemy = guide ? 238 : name === "locked-blind" ? 0 : 122;
  const enemyName = guide ? "Zed" : "Darius";
  const runes = guide ? { primaryPageId: 8100, keystone: 8112, primaryRunes: [8126, 8138, 8106], secondaryPageId: 8200, secondaryRunes: [8226, 8210], shards: [5008, 5008, 5001] } : selection;
  return {
    phase: "ChampSelect", championId: own, championName: guide ? "Ahri" : own ? "Aatrox" : "", position: guide ? "mid" : "top", mode: "ranked",
    source: "lolalytics", sourceLabel: "lolalytics", message: null,
    presets: own ? [{ index: 0, title: guide ? "Electrocute" : "Conqueror", ...runes, winPct: 51.2, play: 41000, spells: [4, 12] }] : [],
    trees: [], shards: [],
    applied: locked ? { name: `Swapper: ${guide ? "Ahri" : "Aatrox"}`, championId: own, ...runes, presetIndex: 0 } : null,
    autoApply: false, importItems: true, canApply: locked, locked,
    spells: { spell1Id: 4, spell2Id: 14, available: [{ id: 4, name: "Flash" }, { id: 12, name: "Teleport" }, { id: 14, name: "Ignite" }], applyWithRunes: true },
    tier: "diamond_plus", tierLabel: "Diamond+", tiers: [], tierSupported: true, tierEmpty: false,
    enemyChampionId: enemy, enemyChampionName: enemy ? enemyName : "", enemies: enemy ? [{ id: enemy, name: enemyName }] : [], enemySlots: 5,
    games: 41000, stale: false, updatedAt: null,
  } as RunesView;
}
