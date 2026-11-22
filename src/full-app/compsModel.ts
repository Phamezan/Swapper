/** Pure Comps logic: coverage assignment, filters, pool helpers. No IO. */
export const ROLES = ["top", "jungle", "mid", "bot", "support"] as const;
export type Role = (typeof ROLES)[number];
export const ROLE_LABEL: Record<Role, string> = { top: "TOP", jungle: "JNG", mid: "MID", bot: "BOT", support: "SUP" };

export type PoolEntry = { championId: number; role: Role; reason: string; games?: number; masteryPoints?: number; masteryLevel?: number; added?: boolean; hidden?: boolean };
export type TeamPlayer = { puuid: string; riotId: string; nickname?: string | null; roles?: Role[]; games?: number; profileIconId?: number; isSelf: boolean; included: boolean; pool: PoolEntry[] | null; mainRole?: Role | null; assignedRole?: Role | null; inLobby?: boolean };
export type RosterMember = { puuid: string; riotId: string; nickname?: string | null; roles: Role[] };
export type CompOption = { championId: number; name: string; core?: boolean };
export type CompSlot = { role: Role; options: CompOption[]; draftOrder: "Pick early" | "Flexible" | "Hold for last" | "Pick together" };
export type Variation = { id: string; style: string; name: string; winCondition: string; tags: string[]; power: { early: number; mid: number; late: number }; weakAgainst: string[]; slots: CompSlot[]; tips: string[]; proExamples: { title: string; detail: string; result?: string; vod?: string; vodStart?: number }[]; sources: string[] };
export type Member = { puuid: string; riotId: string; games?: number };
export type Assignment = { role: Role; champion: CompOption; player: TeamPlayer; entry: PoolEntry } | { role: Role; champion: null; player: null; entry: null };
export type Coverage = { count: number; assignments: Assignment[]; missing: Role[]; playableByMe: boolean };

const roleOf = (e: PoolEntry) => e.role;

const allowedOptions = (slot: CompSlot) => slot.options.some(o => o.core) ? slot.options.filter(o => o.core) : slot.options;

/** Exhaustive assignment (5 slots x <=5 players): maximise filled slots, then total score, deterministic tie-break. */
export function coverageFor(variation: Variation, team: TeamPlayer[]): Coverage {
  const players = team.filter(p => p.included && p.pool);
  type Cand = { opt: CompOption; player: TeamPlayer; entry: PoolEntry; score: number };
  const perSlot: Cand[][] = variation.slots.map(slot => {
    const cands: Cand[] = [];
    for (const opt of allowedOptions(slot)) for (const player of players) {
      const entry = (player.pool ?? []).find(e => e.championId === opt.championId && !e.hidden);
      if (!entry) continue;
      const score = (opt.core ? 100 : 0) + (player.mainRole === slot.role ? 30 : 0) + (roleOf(entry) === slot.role ? 20 : 0)
        + Math.min(20, (entry.games ?? 0)) + Math.min(10, (entry.masteryPoints ?? 0) / 50000);
      cands.push({ opt, player, entry, score });
    }
    return cands.sort((a, b) => b.score - a.score || a.player.riotId.localeCompare(b.player.riotId) || a.opt.championId - b.opt.championId);
  });
  let best: (Cand | null)[] = variation.slots.map(() => null);
  let bestFilled = -1, bestScore = -1;
  const current: (Cand | null)[] = variation.slots.map(() => null);
  const used = new Set<string>();
  const dfs = (i: number, filled: number, score: number) => {
    if (i === variation.slots.length) {
      if (filled > bestFilled || (filled === bestFilled && score > bestScore)) { bestFilled = filled; bestScore = score; best = [...current]; }
      return;
    }
    // Try fills first (sorted by score) so the first optimum wins ties deterministically.
    for (const c of perSlot[i]) {
      if (used.has(c.player.puuid)) continue;
      used.add(c.player.puuid); current[i] = c;
      dfs(i + 1, filled + 1, score + c.score);
      used.delete(c.player.puuid); current[i] = null;
    }
    dfs(i + 1, filled, score); // leave empty
  };
  dfs(0, 0, 0);
  const assignments: Assignment[] = variation.slots.map((slot, i) => {
    const c = best[i];
    return c ? { role: slot.role, champion: c.opt, player: c.player, entry: c.entry } : { role: slot.role, champion: null, player: null, entry: null };
  });
  const missing = assignments.filter(a => !a.champion).map(a => a.role);
  const self = team.find(p => p.isSelf);
  const playableByMe = !!self?.pool && variation.slots.some(s => allowedOptions(s).some(o => self.pool!.some(e => e.championId === o.championId && !e.hidden)));
  return { count: assignments.length - missing.length, assignments, missing, playableByMe };
}

export function peakLabel(v: Variation): string {
  const { early, mid, late } = v.power;
  const peak = Math.max(early, mid, late);
  return peak === late ? "Strong late game" : peak === mid ? "Strong mid game" : "Strong early game";
}
/** Browse: no included teammates. Solo: the only included player is you. Team: an included teammate exists. */
export function teamMode(team: TeamPlayer[]): "browse" | "solo" | "team" {
  const inc = team.filter(p => p.included);
  if (inc.some(p => !p.isSelf)) return "team";
  return inc.length === 1 && inc[0].isSelf ? "solo" : "browse";
}
/** Display name: You, else nickname, else Riot ID game name. */
export function playerLabel(p: { isSelf?: boolean; nickname?: string | null; riotId: string }): string {
  if (p.isSelf) return "You";
  return p.nickname?.trim() || p.riotId.split("#")[0] || "Player";
}

/** Toggle a player in the active set (max 5). Returns the same array when full and adding. */
export function toggleActive(active: string[], puuid: string): string[] {
  if (active.includes(puuid)) return active.filter(id => id !== puuid);
  return active.length >= 5 ? active : [...active, puuid];
}

/** Roster members in the lobby become active (up to 5); manual order is kept. */
export function lobbyActive(rosterPuuids: string[], lobbyPuuids: string[], current: string[]): string[] {
  const roster = new Set(rosterPuuids);
  const next = [...current];
  for (const puuid of lobbyPuuids) if (roster.has(puuid) && !next.includes(puuid) && next.length < 5) next.push(puuid);
  return next;
}

/** Lobby players who are not You and not in the roster yet. */
export function lobbyCandidates<T extends { puuid: string }>(lobby: T[], rosterPuuids: string[], mePuuid: string | null): T[] {
  const roster = new Set(rosterPuuids);
  return lobby.filter(m => m.puuid !== mePuuid && !roster.has(m.puuid));
}

const ROLE_PERMS: Role[][] = (() => {
  const perms = (a: Role[]): Role[][] => a.length < 2 ? [a] : a.flatMap((x, i) => perms(a.filter((_, j) => j !== i)).map(p => [x, ...p]));
  return perms([...ROLES]);
})();

/**
 * Role auto-assign: distinct roles maximising preference rank; manual overrides win.
 * A player the solver could not place on a preferred role keeps their first choice
 * and is flagged in `clash` when that duplicates someone else's role.
 */
export function assignRoles(players: { puuid: string; roles: Role[] }[], overrides: Record<string, Role>): { assigned: Record<string, Role>; clash: string[]; open: Role[] } {
  let best: Role[] = [...ROLES], bestScore = -Infinity;
  for (const perm of ROLE_PERMS) {
    const score = players.reduce((s, p, i) => {
      const o = overrides[p.puuid], rank = p.roles.indexOf(perm[i]);
      return s + (o ? (o === perm[i] ? 1000 : -1000) : rank < 0 ? -100 : 5 - rank);
    }, 0);
    if (score > bestScore) { bestScore = score; best = perm; }
  }
  const assigned: Record<string, Role> = {};
  players.forEach((p, i) => { assigned[p.puuid] = overrides[p.puuid] ?? (p.roles.includes(best[i]) ? best[i] : (p.roles[0] ?? best[i])); });
  const clash = players.filter(a => players.some(b => b.puuid !== a.puuid && assigned[b.puuid] === assigned[a.puuid])).map(p => p.puuid);
  return { assigned, clash, open: ROLES.filter(r => !Object.values(assigned).includes(r)) };
}

/** Comps are tailored only with exactly five active players. */
export const isTailored = (activeCount: number) => activeCount === 5;

/**
 * Single-swap sub hint: swapping one active non-self player for one inactive
 * roster member makes this comp 5/5. Never swaps You out. Null otherwise.
 */
export function subHintFor(variation: Variation, team: TeamPlayer[]): { out: TeamPlayer; inn: TeamPlayer } | null {
  const active = team.filter(p => p.included);
  if (active.length !== 5 || coverageFor(variation, team).count === 5) return null;
  for (const out of active) {
    if (out.isSelf) continue;
    for (const inn of team) {
      if (inn.included || !inn.pool) continue;
      const swapped = team.map(p => p.puuid === out.puuid ? { ...p, included: false } : p.puuid === inn.puuid ? { ...p, included: true } : p);
      if (coverageFor(variation, swapped).count === 5) return { out, inn };
    }
  }
  return null;
}

/**
 * Round-2 filter (13A): is this comp playable for the current active set?
 * With 5 active it is playable when the five cover it (5/5) or one swap fixes it.
 * With fewer active it is playable when the active players cover at least one pick.
 * With nobody active nothing is highlighted.
 */
export function compPlayable(cov: Coverage, hasHint: boolean, activeCount: number): boolean {
  if (activeCount <= 0) return false;
  if (activeCount === 5) return cov.count === 5 || hasHint;
  return cov.count > 0;
}

/** Stable partition: playable first (in the given order), the rest after. */
export function partitionPlayable<T>(items: readonly T[], isPlayable: (item: T) => boolean): { playable: T[]; rest: T[] } {
  const playable: T[] = [], rest: T[] = [];
  for (const item of items) (isPlayable(item) ? playable : rest).push(item);
  return { playable, rest };
}

/** Role for a manually added champion: its own main role, else your main role, else mid. */
export function addedChampionRole(champRole: Role | undefined, mainRole: Role | null | undefined): Role {
  return champRole ?? mainRole ?? "mid";
}

export function applyOverrides(pool: PoolEntry[], added: { championId: number; role: Role; name?: string }[], hidden: number[]): PoolEntry[] {
  const hiddenSet = new Set(hidden);
  const base: PoolEntry[] = pool.map(e => ({ ...e, hidden: hiddenSet.has(e.championId) }));
  for (const a of added) if (!base.some(e => e.championId === a.championId)) base.push({ championId: a.championId, role: a.role, reason: "Added by you", added: true });
  return base;
}
export function isOlderMeta(answerPatch: string, currentPatch: string): boolean {
  const parse = (p: string) => p.split(".").map(Number);
  const [aMaj, aMin] = parse(answerPatch), [cMaj, cMin] = parse(currentPatch);
  if (aMaj !== cMaj) return cMaj - aMaj >= 1;
  return cMin - aMin > 2;
}
