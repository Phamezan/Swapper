// The Champions page's "Auto" role: every champion once, at the role it is
// played in most. Pure so node:test can cover it.
import type { TierRow } from "../runes/types";

export const ROLES = ["top", "jungle", "mid", "adc", "support"] as const;
const TIER_ORDER = ["S+", "S", "A", "B", "C", "D"];

export type AutoRow = TierRow & { role: string };

/** Each champion's most-played role across the per-role tier lists (by games). */
export function mainRoles(lists: Record<string, TierRow[]>): Map<number, string> {
  const best = new Map<number, { role: string; games: number }>();
  for (const role of ROLES) {
    for (const row of lists[role] ?? []) {
      const current = best.get(row.championId);
      if (!current || row.games > current.games) best.set(row.championId, { role, games: row.games });
    }
  }
  return new Map([...best].map(([id, { role }]) => [id, role]));
}

/** One row per champion at its main role, strongest tier first, then win rate. */
export function autoRows(lists: Record<string, TierRow[]>): AutoRow[] {
  const roles = mainRoles(lists);
  const rows = ROLES.flatMap(role => (lists[role] ?? []).filter(row => roles.get(row.championId) === role).map(row => ({ ...row, role })));
  const tierIndex = (tier: string) => { const i = TIER_ORDER.indexOf(tier); return i < 0 ? TIER_ORDER.length : i; };
  return rows.sort((a, b) => tierIndex(a.tier) - tierIndex(b.tier) || b.winPct - a.winPct);
}

/** The champions played most in recent games, each with the role it was played in most. */
export function mostPlayed(games: { championId: number; role: string }[], limit = 3): { championId: number; role: string; games: number }[] {
  const byChampion = new Map<number, Map<string, number>>();
  for (const { championId, role } of games) {
    const roles = byChampion.get(championId) ?? new Map<string, number>();
    roles.set(role, (roles.get(role) ?? 0) + 1);
    byChampion.set(championId, roles);
  }
  return [...byChampion]
    .map(([championId, roles]) => ({ championId, games: [...roles.values()].reduce((a, b) => a + b, 0), role: [...roles].sort((a, b) => b[1] - a[1])[0][0] }))
    .sort((a, b) => b.games - a.games || a.championId - b.championId)
    .slice(0, limit)
    .map(pick => ({ ...pick, role: (ROLES as readonly string[]).includes(pick.role) ? pick.role : "auto" }));
}
