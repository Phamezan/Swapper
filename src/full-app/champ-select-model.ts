import type { Applied, CounterRow, RunesView, Selection, TierRow } from "../runes/types";
import type { RecommendedPick } from "./types";

export type DraftMember = { championId: number; position: string; local: boolean; locked: boolean };
export type Draft = { allies: DraftMember[]; enemies: DraftMember[]; bans: number[]; timer: { phase: string; remainingMs: number } | null };
export type PageMode = "idle" | "planning" | "build";
export type SuggestionTab = "counters" | "recommended" | "meta";
export type Suggestion = { championId: number; name: string; winPct: number; games: number; flex?: boolean };
export type ApplyStatus = { runes: boolean; spells: boolean; items: boolean };
type Skill = "Q" | "W" | "E" | "R";

/** Planning until your pick is locked in; un-hovering or swapping before lock stays in planning.
 *  ARAM and Arena have no picks to plan (and no lock-in), so they go straight to the build. */
export function pageMode(view: Pick<RunesView, "phase" | "championId" | "locked" | "mode"> | null | undefined): PageMode {
  if (view?.phase !== "ChampSelect") return "idle";
  const noDraft = view.mode === "aram" || view.mode === "arena";
  return (view.locked || noDraft) && view.championId > 0 ? "build" : "planning";
}

export const defaultTab = (enemyId: number): SuggestionTab => enemyId > 0 ? "counters" : "recommended";

const LCU_ROLES: Record<string, string> = { top: "top", jungle: "jungle", middle: "mid", bottom: "adc", utility: "support" };
/** LCU `assignedPosition` ("middle", "utility") to Swapper's role slug ("mid", "support"). */
export const roleSlug = (position: string) => LCU_ROLES[position.trim().toLowerCase()] ?? "";

/** The revealed enemy in your lane: same role, or the only revealed enemy in blind pick. 0 when unknown. */
export function enemyLaner(draft: Draft | null, role: string): number {
  const revealed = (draft?.enemies ?? []).filter(member => member.championId > 0);
  if (role && role !== "none") return revealed.find(member => roleSlug(member.position) === role)?.championId ?? 0;
  return revealed.length === 1 && !revealed[0].position ? revealed[0].championId : 0;
}

/** Banned and locked champions, which can no longer be hovered. */
export function takenIds(draft: Draft | null): Set<number> {
  const locked = [...(draft?.allies ?? []), ...(draft?.enemies ?? [])].filter(member => member.locked && !member.local);
  return new Set([...(draft?.bans ?? []), ...locked.map(member => member.championId)].filter(id => id > 0));
}

/** The enemy laner's worst reliable matchups, as your win rate into them. */
export function counterPicks(rows: CounterRow[], taken: Set<number>, limit = 8): Suggestion[] {
  return rows.filter(row => !row.lowSample && !taken.has(row.championId))
    .sort((a, b) => a.winPct - b.winPct || b.games - a.games)
    .slice(0, limit)
    .map(row => ({ championId: row.championId, name: row.name, winPct: 100 - row.winPct, games: row.games }));
}

const TIER_ORDER = ["S+", "S", "A", "B", "C", "D"];
export function metaPicks(rows: TierRow[], taken: Set<number>, limit = 8): Suggestion[] {
  const rank = (tier: string) => { const index = TIER_ORDER.indexOf(tier); return index < 0 ? TIER_ORDER.length : index; };
  return rows.filter(row => !taken.has(row.championId))
    .sort((a, b) => rank(a.tier) - rank(b.tier) || b.winPct - a.winPct)
    .slice(0, limit)
    .map(row => ({ championId: row.championId, name: row.name, winPct: row.winPct, games: row.games }));
}

export function recommendedSuggestions(picks: RecommendedPick[], names: Map<number, string>, taken: Set<number>): Suggestion[] {
  return picks.filter(pick => !taken.has(pick.championId))
    .map(pick => ({ championId: pick.championId, name: names.get(pick.championId) ?? `Champion ${pick.championId}`, winPct: pick.winRate, games: pick.games, flex: pick.flex }));
}

const sameSet = (a: number[], b: number[]) => a.length === b.length && a.every(id => b.includes(id));
export function samePage(applied: Applied | null | undefined, selection: Selection | null | undefined): boolean {
  if (!applied || !selection) return false;
  return applied.primaryPageId === selection.primaryPageId && applied.secondaryPageId === selection.secondaryPageId
    && applied.keystone === selection.keystone && sameSet(applied.primaryRunes, selection.primaryRunes)
    && sameSet(applied.secondaryRunes, selection.secondaryRunes) && applied.shards.join() === selection.shards.join();
}

export const itemSetKey = (championId: number, items: number[]) => items.length ? `${championId}:${items.join(",")}` : "";

/** What the client already has for the shown build. Items are only known from this page's own import. */
export function applyStatus(client: RunesView | null | undefined, championId: number, selection: Selection | null, spells: number[], itemsKey: string, importedKey: string | null): ApplyStatus {
  const own = client?.phase === "ChampSelect" && client.championId === championId;
  return {
    runes: own && client.applied?.championId === championId && samePage(client.applied, selection),
    spells: own && spells.length === 2 && client.spells.spell1Id === spells[0] && client.spells.spell2Id === spells[1],
    items: !!itemsKey && itemsKey === importedKey,
  };
}

/** Q/W/E/R rows over levels 1-18; a cell holds its level when that skill is taken then. */
export function skillGrid(order: string | string[] | null | undefined, levels = 18): { skill: Skill; cells: (number | null)[] }[] {
  const steps = [...(Array.isArray(order) ? order.join("") : order ?? "").toUpperCase()].slice(0, levels);
  return (["Q", "W", "E", "R"] as Skill[]).map(skill => ({ skill, cells: Array.from({ length: levels }, (_, i) => steps[i] === skill ? i + 1 : null) }));
}

/** The max order, e.g. ["Q", "E", "W"]: the reported priority, else the order basic skills fill up in. */
export function maxOrder(order: string | string[] | null | undefined, priority?: string | null): Skill[] {
  const reported = [...(priority ?? "").toUpperCase()].filter((key): key is Skill => "QWE".includes(key));
  if (reported.length === 3 && new Set(reported).size === 3) return reported;
  const steps = [...(Array.isArray(order) ? order.join("") : order ?? "").toUpperCase()];
  if (steps.length < 9) return [];
  const stats = (["Q", "W", "E"] as Skill[]).map(skill => ({ skill, count: steps.filter(step => step === skill).length, last: steps.lastIndexOf(skill) }));
  return stats.sort((a, b) => b.count - a.count || a.last - b.last).map(stat => stat.skill);
}

/** Whole seconds left in the draft phase, counting down from when the draft was read. */
export function secondsLeft(timer: Draft["timer"], readAt: number, now: number): number | null {
  if (!timer || timer.remainingMs <= 0) return null;
  return Math.max(0, Math.ceil((timer.remainingMs - (now - readAt)) / 1000));
}
