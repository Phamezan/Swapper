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

/** The share of a champion's games played in each of `ROLES`; zeros when it is in no list. */
export function roleShares(championId: number, lists: Record<string, TierRow[]>): number[] {
  const games = ROLES.map(role => (lists[role] ?? []).find(row => row.championId === championId)?.games ?? 0);
  const total = games.reduce((a, b) => a + b, 0);
  return games.map(count => total ? count / total : 0);
}

/** One role per team member, each role at most once: the most likely assignment, from each
 *  champion's share of its games per role (Karthus + Shyvana is Karthus bot, Shyvana jungle).
 *  A member's `role` (a position the client reported) is kept; members without a champion or
 *  data fill whatever is left. Empty when more than five members or reported roles clash. */
export function assignRoles(members: { championId: number; role: string }[], lists: Record<string, TierRow[]>): string[] {
  // A role it is never listed in is unlikely, not impossible.
  const shares = members.map(({ championId }) => {
    const shares = roleShares(championId, lists);
    const known = shares.some(Boolean);
    return shares.map(share => Math.log(known ? Math.max(share, 1e-4) : 1 / ROLES.length));
  });
  let best: { score: number; roles: string[] } = { score: -Infinity, roles: [] };
  const search = (index: number, roles: string[], score: number) => {
    if (index === members.length) {
      if (score > best.score) best = { score, roles };
      return;
    }
    ROLES.forEach((role, r) => {
      if (roles.includes(role) || (members[index].role && members[index].role !== role)) return;
      search(index + 1, [...roles, role], score + shares[index][r]);
    });
  };
  if (members.length <= ROLES.length) search(0, [], 0);
  return best.roles;
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

/** The champion pool for quick picks: champions from recent games first (they carry the role played), then the rest of
 *  the top mastery champions, most recently played first, as "auto" (their main role is resolved on open). */
export function championPool(recent: { championId: number; role: string }[], mastery: { championId: number; lastPlayed: number }[], limit = 10): { championId: number; role: string }[] {
  const seen = new Set(recent.map(pick => pick.championId));
  const rest = [...mastery].sort((a, b) => b.lastPlayed - a.lastPlayed).filter(pick => !seen.has(pick.championId)).map(pick => ({ championId: pick.championId, role: "auto" }));
  return [...recent, ...rest].slice(0, limit);
}
