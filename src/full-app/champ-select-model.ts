import type { Applied, CounterRow, RunesView, Selection, TierRow } from "../runes/types";
import type { RecommendedPick } from "./types";

export type DraftMember = { championId: number; position: string; local: boolean; locked: boolean };
export type Draft = { allies: DraftMember[]; enemies: DraftMember[]; bans: number[]; timer: { phase: string; remainingMs: number } | null };
export type PageMode = "idle" | "planning" | "build";
export type SuggestionTab = "all" | "recommended" | "counters";
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

/** Below this share of a role's games a champion does not play it (as `FLEX_MIN_PICK_PCT` in blind.rs). */
const MIN_ROLE_PICK_PCT = 1;

/** The revealed enemies who may be in your lane: the one in your role, or the only revealed enemy in blind pick.
 *  Ranked hides enemy positions, so then it is every enemy who plays your role in `roleRows` (your role's tier list),
 *  likeliest first, for you to choose between. Empty when unknown. */
export function enemyLaners(draft: Draft | null, role: string, roleRows: TierRow[] = []): number[] {
  const revealed = (draft?.enemies ?? []).filter(member => member.championId > 0);
  if (!role || role === "none") return revealed.length === 1 && !revealed[0].position ? [revealed[0].championId] : [];
  const assigned = revealed.find(member => roleSlug(member.position) === role)?.championId;
  if (assigned) return [assigned];
  const unplaced = new Set(revealed.filter(member => !member.position).map(member => member.championId));
  return roleRows.filter(row => unplaced.has(row.championId) && row.pickPct >= MIN_ROLE_PICK_PCT)
    .sort((a, b) => b.pickPct - a.pickPct).map(row => row.championId);
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
    .map(row => ({ championId: row.championId, name: row.name, winPct: 100 - row.winPct, games: row.games + (row.prevGames ?? 0) }));
}

/** Every champion A to Z, with the role's win rate where the tier list has one. */
export function allChampions(names: Map<number, string>, rows: TierRow[], taken: Set<number>): Suggestion[] {
  const stats = new Map(rows.map(row => [row.championId, row]));
  return [...names].filter(([id]) => !taken.has(id)).sort(([, a], [, b]) => a.localeCompare(b))
    .map(([championId, name]) => {
      const row = stats.get(championId);
      return { championId, name, winPct: row?.winPct ?? NaN, games: row?.games ?? 0 };
    });
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
  const steps = completeSkillOrder([...(Array.isArray(order) ? order.join("") : order ?? "").toUpperCase()], levels).slice(0, levels);
  return (["Q", "W", "E", "R"] as Skill[]).map(skill => ({ skill, cells: Array.from({ length: levels }, (_, i) => steps[i] === skill ? i + 1 : null) }));
}

/** Builds report 15 levels; the rest follows League's rules: the ultimate at 6, 11 and 16, each basic
 *  skill caps at 5 points and fills in max order. Short or unclear orders are left as reported. */
export function completeSkillOrder(steps: string[], levels = 18): string[] {
  const priority = maxOrder(steps);
  if (steps.length < 15 || priority.length !== 3 || steps.some(step => !"QWER".includes(step))) return steps;
  const done = [...steps];
  const count = (skill: string) => done.filter(step => step === skill).length;
  while (done.length < levels) {
    const level = done.length + 1;
    const ultimates = level >= 16 ? 3 : level >= 11 ? 2 : level >= 6 ? 1 : 0;
    const next = count("R") < ultimates ? "R" : priority.find(skill => count(skill) < 5);
    if (!next) break;
    done.push(next);
  }
  return done;
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

const searchKey = (text: string) => text.toLowerCase().replace(/[^a-z0-9]/g, "");
/** Champions whose name matches `query` (ignoring case, spaces and punctuation),
 *  names starting with it first, minus taken ones. */
export function searchChampions(names: Map<number, string>, query: string, taken: Set<number>, limit = 16): { championId: number; name: string }[] {
  const q = searchKey(query);
  if (!q) return [];
  return [...names].filter(([id, name]) => !taken.has(id) && searchKey(name).includes(q))
    .sort(([, a], [, b]) => Number(!searchKey(a).startsWith(q)) - Number(!searchKey(b).startsWith(q)) || a.localeCompare(b))
    .slice(0, limit).map(([championId, name]) => ({ championId, name }));
}
