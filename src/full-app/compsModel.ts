/** Pure Comps logic: coverage assignment, filters, pool helpers. No IO. */
export const ROLES = ["top", "jungle", "mid", "bot", "support"] as const;
export type Role = (typeof ROLES)[number];
export const ROLE_LABEL: Record<Role, string> = { top: "TOP", jungle: "JNG", mid: "MID", bot: "BOT", support: "SUP" };

export type PoolEntry = { championId: number; role: Role; reason: string; games?: number; masteryPoints?: number; masteryLevel?: number; added?: boolean; hidden?: boolean };
export type TeamPlayer = { puuid: string; riotId: string; games?: number; profileIconId?: number; isSelf: boolean; included: boolean; pool: PoolEntry[] | null; mainRole?: Role | null };
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
/** Role for a manually added champion: its own main role, else your main role, else mid. */
export function addedChampionRole(champRole: Role | undefined, mainRole: Role | null | undefined): Role {
  return champRole ?? mainRole ?? "mid";
}

/** Team source: lobby (>=2) > saved > me + backend suggestions (or snapshot fallback), then added, cap 5. */
export function selectTeamMembers(input: {
  lobby: Member[]; saved: Member[]; me: Member | null;
  suggested: { puuid: string; riotId: string; games: number }[];
  snapshotGames: Parameters<typeof suggestTeammates>[0];
  added: Member[];
}): { members: Member[]; source: "lobby" | "saved" | "auto" } {
  let members: Member[] = [];
  let source: "lobby" | "saved" | "auto" = "auto";
  if (input.lobby.length >= 2) { members = [...input.lobby]; source = "lobby"; }
  else if (input.saved.length) { members = [...input.saved]; source = "saved"; }
  else if (input.me) {
    members = [input.me];
    const fromBackend = input.suggested ?? [];
    const fallback = fromBackend.length ? [] : suggestTeammates(input.snapshotGames ?? []).filter(s => s.puuid).map(s => ({ puuid: s.puuid, riotId: s.riotId, games: s.count }));
    for (const s of (fromBackend.length ? fromBackend : fallback)) if (s.puuid !== input.me.puuid && members.length < 5) members.push({ puuid: s.puuid, riotId: s.riotId, games: s.games });
  }
  for (const a of input.added) if (!members.some(m => m.puuid === a.puuid) && members.length < 5) members.push(a);
  return { members: members.slice(0, 5), source };
}
/** Queues whose games count as playing together as a team: Ranked Flex (5-stacks) and Clash. */
export const TEAM_QUEUES = ["flex", "clash"];
/** Teammates on the user's team in 3+ of their recent Ranked Flex or Clash games. games: snapshot games with players[]. */
export function suggestTeammates(games: { queue?: string; players: { riotId: string; ally: boolean; me: boolean; puuid?: string }[] }[], limit = 20): { riotId: string; puuid: string; count: number }[] {
  const counts = new Map<string, { riotId: string; puuid: string; count: number }>();
  for (const g of games.filter(g => TEAM_QUEUES.includes(g.queue ?? "")).slice(0, limit)) for (const p of g.players ?? []) {
    if (p.me || !p.ally) continue;
    const key = p.puuid || p.riotId;
    const cur = counts.get(key) ?? { riotId: p.riotId, puuid: p.puuid ?? "", count: 0 };
    cur.count++; counts.set(key, cur);
  }
  return [...counts.values()].filter(c => c.count >= 3).sort((a, b) => b.count - a.count);
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
