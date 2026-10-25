// Mocked Tauri IPC for the dev-only previews (champ-select-preview, ../flyout-preview).
// Icons and the rune catalog come from Data Dragon; everything else is fixture data.
import { mockIPC } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import type { RuneTree, RunesView } from "../runes/types";
import { fixtureDraft, fixtureView, type FixtureName } from "./champ-select-fixtures";
import { takenIds } from "./champ-select-model";

const DD = "https://ddragon.leagueoflegends.com/cdn";
const VERSION = "16.21.1";
const CHAMPIONS: Record<number, [string, string]> = {
  266: ["Aatrox", "Aatrox"], 122: ["Darius", "Darius"], 103: ["Ahri", "Ahri"], 64: ["Lee Sin", "LeeSin"], 222: ["Jinx", "Jinx"],
  157: ["Yasuo", "Yasuo"], 777: ["Yone", "Yone"], 893: ["Aurora", "Aurora"], 875: ["Sett", "Sett"], 233: ["Briar", "Briar"], 902: ["Milio", "Milio"],
  67: ["Vayne", "Vayne"], 133: ["Quinn", "Quinn"], 17: ["Teemo", "Teemo"], 85: ["Kennen", "Kennen"], 150: ["Gnar", "Gnar"], 126: ["Jayce", "Jayce"],
  10: ["Kayle", "Kayle"], 54: ["Malphite", "Malphite"], 2: ["Olaf", "Olaf"], 86: ["Garen", "Garen"], 24: ["Jax", "Jax"], 58: ["Renekton", "Renekton"],
  164: ["Camille", "Camille"], 82: ["Mordekaiser", "Mordekaiser"], 516: ["Ornn", "Ornn"], 897: ["K'Sante", "KSante"], 98: ["Shen", "Shen"],
};
const ITEMS: Record<number, string> = { 1055: "Doran's Blade", 2003: "Health Potion", 3047: "Plated Steelcaps", 3111: "Mercury's Treads", 6692: "Eclipse", 3053: "Sterak's Gage", 3071: "Black Cleaver", 6333: "Death's Dance", 3156: "Maw of Malmortius", 3065: "Spirit Visage", 3026: "Guardian Angel", 6610: "Sundered Sky", 3074: "Ravenous Hydra" };
const SPELLS: Record<number, string> = { 4: "SummonerFlash", 12: "SummonerTeleport", 14: "SummonerDot", 6: "SummonerHaste" };
const item = (id: number) => ({ id, name: ITEMS[id] ?? "Item" });
const option = (id: number, slot: number, winPct: number, games: number) => ({ ...item(id), slot, winPct, games });
const SHARDS = [[5008, 5005, 5007], [5008, 5010, 5001], [5011, 5013, 5001]];
const page = (primaryPageId: number, keystone: number, primaryRunes: number[], secondaryPageId: number, secondaryRunes: number[], shards: number[]) => ({ primaryPageId, keystone, primaryRunes, secondaryPageId, secondaryRunes, shards });
const build = (items: number[], winPct: number, games: number, skillOrder: string, extra = {}) => ({
  starters: [{ ...item(1055), count: 1 }, { ...item(2003), count: 1 }], items: items.map(item), options: [option(6333, 4, 53.1, 4200), option(3156, 4, 52.4, 1900), option(3065, 5, 54.0, 2100), option(3026, 6, 55.2, 1500)],
  games, coreGames: Math.round(games / 3), coreLen: 3, coreWinPct: winPct, stale: false, updatedAt: null, skillPriority: null, skillOrder, ...extra,
});
const ORDER = "QEWQQRQEQERWEEWRWW";
const counters = [[67, 46.1, 2400], [133, 46.8, 1300], [17, 47.2, 3100], [85, 47.5, 900], [150, 48.1, 2100], [126, 48.4, 1800], [10, 48.6, 1500], [54, 49.0, 2800], [2, 49.4, 700], [98, 49.8, 1100]];
const tiers = [[266, "S+", 51.6], [24, "S", 51.4], [875, "S+", 52.2], [86, "S", 51.1], [58, "A", 50.6], [164, "S", 50.9], [82, "A", 51.0], [516, "A", 50.4], [897, "B", 49.2], [54, "A", 51.3], [122, "S", 50.8]];

let fixture: FixtureName = (new URLSearchParams(location.search).get("fixture") as FixtureName) || "planning";
let selectedChampion = fixtureView(fixture).championId;
let applied: RunesView["applied"] = new URLSearchParams(location.search).has("rune-fail") ? null : fixtureView(fixture).applied;
let spells = fixtureView(fixture).spells;
/** Commands a page adds to, or overrides in, the champion select ones (the flyout preview). */
export const extra: Record<string, (args: Record<string, any>) => unknown> = {};
export const listeners = new Set<() => void>();
export const current = (): RunesView => ({ ...fixtureView(fixture), championId: selectedChampion, championName: CHAMPIONS[selectedChampion]?.[0] ?? "", trees: catalog?.trees ?? [], shards: catalog?.shards ?? [], applied, spells });
const currentDraft = () => {
  const draft = fixtureDraft(fixture);
  draft.allies = draft.allies.map(member => member.local ? { ...member, championId: selectedChampion } : member);
  const params = new URLSearchParams(location.search);
  if (params.has("turn")) draft.canLock = false;
  if (params.get("turn") === "intent") draft.timer!.phase = "PLANNING";
  draft.pickable = Object.keys(CHAMPIONS).map(Number).filter(id => !params.has("unavailable") || id !== 266);
  return draft;
};
const change = (next: FixtureName, championId = selectedChampion) => { fixture = next; selectedChampion = championId; applied = fixtureView(next).applied; spells = fixtureView(next).spells; listeners.forEach(listener => listener()); void emit("draft_changed", currentDraft()); };
let catalog: { trees: RuneTree[]; shards: { kind: string; label: string; runes: { id: number; name: string; winPct: null; pickPct: null; play: number }[] }[] } | null = null;
const runeIcons = new Map<number, string>(Object.entries({ 5008: "AdaptiveForce", 5005: "AttackSpeed", 5007: "CDRScaling", 5010: "MovementSpeed", 5001: "HealthPlus", 5011: "HealthScaling", 5013: "Tenacity" }).map(([id, name]) => [Number(id), `${DD}/img/perk-images/StatMods/StatMods${name}Icon.png`]));
const ready = fetch(`${DD}/${VERSION}/data/en_US/runesReforged.json`).then(r => r.json()).then((trees: { id: number; name: string; slots: { runes: { id: number; name: string; icon: string }[] }[] }[]) => {
  const rune = (r: { id: number; name: string; icon: string }) => { runeIcons.set(r.id, `${DD}/img/${r.icon}`); return { id: r.id, name: r.name, winPct: null, pickPct: null, play: 0 }; };
  catalog = {
    trees: trees.map(tree => ({ id: tree.id, name: tree.name, keystones: tree.slots[0].runes.map(rune), rows: tree.slots.slice(1).map((slot, i) => ({ kind: "row", label: `Row ${i + 1}`, runes: slot.runes.map(rune) })) })),
    shards: SHARDS.map((row, i) => ({ kind: "shard", label: `Shard ${i + 1}`, runes: row.map(id => ({ id, name: `Shard ${id}`, winPct: null, pickPct: null, play: 0 })) })),
  };
}).catch(() => {});

mockIPC(async (cmd, args) => {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const a = (args ?? {}) as Record<string, any>;
  if (extra[cmd]) return extra[cmd](a);
  switch (cmd) {
    case "get_runes": return current();
    case "full_app_draft": return currentDraft();
    case "get_champion_list": return Object.entries(CHAMPIONS).map(([id, [name]]) => ({ id: Number(id), name }));
    case "champion_icon": return `${DD}/${VERSION}/img/champion/${CHAMPIONS[a.id]?.[1] ?? "Aatrox"}.png`;
    // Same crops as the client's art: 380 px tiles and the centred splash.
    case "champion_art": return `${DD}/img/champion/${a.kind === "splash" ? "splash" : a.kind === "loading" ? "loading" : "tiles"}/${CHAMPIONS[a.id]?.[1] ?? "Aatrox"}_0.jpg`;
    case "item_icon": return `${DD}/${VERSION}/img/item/${a.id}.png`;
    case "spell_icon": return `${DD}/${VERSION}/img/spell/${SPELLS[a.id] ?? "SummonerFlash"}.png`;
    case "rune_icon": await ready; if (runeIcons.has(a.id)) return runeIcons.get(a.id); throw new Error("no icon");
    case "role_icon": return `https://raw.communitydragon.org/latest/plugins/rcp-fe-lol-static-assets/global/default/svg/position-${a.role}.svg`;
    case "full_app_rune_catalog": await ready; if (catalog) return catalog; throw new Error("Rune catalog unavailable");
    case "full_app_abilities": return ["Q", "W", "E", "R"].map(key => ({ key, name: `Aatrox ${key}`, icon: `${DD}/${VERSION}/img/spell/Aatrox${key === "R" ? "R" : key}.png` }));
    case "champion_abilities": return { championId: a.championId, tooltipsReady: true, passive: { name: "Passive", iconPath: null, description: "This champion's innate effect.", tooltip: null }, spells: ["Q", "W", "E", "R"].map(key => ({ key, name: `Enemy ${key}`, iconPath: null, description: "Deals damage to enemies.", tooltip: key === "Q" ? "Deals 60/90/120/150/180 (+50% AP) magic damage." : null, cooldowns: key === "R" ? [120, 100, 80] : [12, 11, 10, 9, 8], costs: [], ranges: [], charges: 0, recharge: [] })), summoners: [] };
    case "get_champion_overview": return { championId: a.championId, position: "top", stats: null, damage: null, unavailable: false, build: { runes: page(8000, 8010, [9111, 9104, 8299], 8300, [8304, 8347], [5005, 5008, 5001]), spells: [4, 12], items: build([6692, 3053, 3071, 6333, 3065, 3026], 52.8, 18200, ORDER), skillOrder: ORDER } };
    case "get_keystone_build": return { ...build([6692, 3053, 3071, 6333, 3065, 3026], 52.8, 18200, ORDER), highestWin: { preset: { index: 1, title: "Grasp", ...page(8400, 8437, [8446, 8429, 8451], 8000, [9111, 9104], [5005, 5008, 5001]), winPct: 54.1, play: 6100, spells: [4, 14] }, build: build([6610, 3053, 3071, 3065, 3026, 6333], 54.3, 6100, "QWEQQRQEQERWEEWRWW") }, highestWinMatchesPick: false };
    case "get_matchup": return { enemyChampionId: a.enemyChampionId, enemyName: "Darius", fallback: false, stats: { winPct: 47.6, avgWinPct: 50.4, delta: -2.8, games: 3150, patch: "16.21" }, preset: { index: 0, title: "Conqueror", ...page(8000, 8010, [9111, 9105, 8299], 8400, [8444, 8242], [5005, 5008, 5011]), winPct: 47.6, play: 3150, spells: [4, 12] }, build: build([6692, 3111, 3053, 3071, 3065, 3026], 48.9, 3150, "QEQWQRQEQERWEEWRWW") };
    case "full_app_matchups": return { championId: a.championId, championName: "Darius", position: "top", bestInto: [], unavailable: false, stale: false, updatedAt: null, beats: counters.map(([id, winPct, games]) => ({ championId: id, name: CHAMPIONS[id][0], winPct, delta: winPct - 50.4, games, lowSample: false })) };
    case "get_tier_list": return { unavailable: false, stale: false, updatedAt: null, rows: tiers.map(([id, tier, winPct]) => ({ championId: id, name: CHAMPIONS[id as number][0], tier, winPct, pickPct: 6, banPct: 3, games: 20000 })) };
    case "recommended_picks": return tiers.slice(0, 8).map(([championId, , winRate]) => ({ championId, winRate, games: 20000, flex: championId === 875 }));
    case "get_pro_builds": {
      if (new URLSearchParams(location.search).has("pro-error")) throw new Error("Preview: pro provider unavailable");
      const proPage = a.page ?? 1;
      return { championId: 266, position: "top", role: "top", page: proPage, hasMore: proPage === 1, unavailable: false, message: null, stale: false, updatedAt: null,
        matches: new URLSearchParams(location.search).has("pro-empty") ? [] : [1, 2, 3, 4].map(n => { const i = n + (proPage - 1) * 4; return {
          matchId: i, proName: `Pro player ${String.fromCharCode(64 + i)}`, team: `Team ${i}`, league: "LCK", region: "KR", role: "top", roleLabel: "Top", win: i % 2 === 1, playedAgo: `${i * 3}h ago`, patch: "16.21",
          ...page(8400, 8437, [8446, 8429, 8451], 8000, [9111, 9104], [5005, 5008, 5001]), kills: 4 + i, deaths: 2, assists: 6, spells: [4, 12],
          finalItems: [6610, 3053, 3071, 3047, 6333, 3026].map(item), completedItems: [6610, 3053, 3071, 6333].map(item), skillOrder: "QWEQQRQEQERWEEWRWW".split(""),
          itemPath: [{ minute: 0, items: [{ ...item(1055), count: 1 }, { ...item(2003), count: 1 }] }], otp: false,
        }; }) };
    }
    case "full_app_hover_champion":
      if (!currentDraft().pickable?.includes(a.championId) || takenIds(currentDraft()).has(a.championId)) throw new Error("That champion is not currently pickable.");
      change("hover", a.championId); return null;
    case "full_app_lock_in": {
      document.body.dataset.lockCalls = String(Number(document.body.dataset.lockCalls ?? 0) + 1);
      if (new URLSearchParams(location.search).has("lock-delay")) await new Promise(resolve => setTimeout(resolve, 200));
      if (new URLSearchParams(location.search).has("lock-fail")) throw new Error("Lock in failed. Try again.");
      if (!currentDraft().canLock || selectedChampion !== a.championId) throw new Error("Wait for your pick turn to lock in.");
      change("locked"); return null;
    }
    case "apply_rune_page": if (new URLSearchParams(location.search).has("rune-fail")) throw new Error("Your rune pages are full (25 of 25). Delete a page in League Client, then try again."); applied = { name: "Swapper", championId: 266, ...a.selection, presetIndex: null }; if (a.spells) spells = { ...spells, spell1Id: a.spells[0], spell2Id: a.spells[1] }; return null;
    case "apply_spell": spells = a.slot === "d" ? { ...spells, spell1Id: a.spellId } : { ...spells, spell2Id: a.spellId }; return null;
    case "import_full_item_build": return null;
    default: throw new Error(`Not mocked: ${cmd}`);
  }
}, { shouldMockEvents: true });
(globalThis as { isTauri?: boolean }).isTauri = true;
