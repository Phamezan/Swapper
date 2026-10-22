import type { MatchDetailPlayer, MatchDetailTeam, MatchDetailView } from "./types";

export type StatValue = number | boolean | null;
export type MatchPlayer = MatchDetailPlayer & { stats?: Record<string, number | boolean> };
export type TimelineEvent = { timestamp: number; teamId: number; label: string; killerId?: number; victimId?: number };
export type Timeline = { frames: { timestamp: number; goldDiff: number; blueGold?: number; redGold?: number }[]; events: TimelineEvent[] };
export type MatchView = Omit<MatchDetailView, "players"> & { players: MatchPlayer[]; timeline?: Timeline | null };
type Direction = "high" | "low" | "none";
export type StatRow = { key: string; label: string; values: StatValue[]; leaders: number[]; format?: "ratio" | "time" };
export type StatGroup = { label: string; rows: StatRow[] };

/** No leader can be claimed when a value is missing or everyone is equal. */
export function bestValues(values: StatValue[], direction: Direction): number[] {
  if (direction === "none" || values.length < 2 || values.some(v => v == null)) return [];
  const numbers = values.map(Number);
  if (numbers.every(v => v === numbers[0])) return [];
  const best = direction === "low" ? Math.min(...numbers) : Math.max(...numbers);
  return numbers.flatMap((v, i) => v === best ? [i] : []);
}

type Definition = [key: string, label: string, direction?: Direction, format?: StatRow["format"]];
const STAT_GROUPS: [string, Definition[]][] = [
  ["Combat", [
    ["kills", "Kills"], ["deaths", "Deaths", "low"], ["assists", "Assists"], ["kda", "KDA", "high", "ratio"],
    ["largestKillingSpree", "Largest killing spree"], ["killingSprees", "Killing sprees"], ["largestMultiKill", "Largest multi kill"],
    ["doubleKills", "Double kills"], ["tripleKills", "Triple kills"], ["quadraKills", "Quadra kills"], ["pentaKills", "Penta kills"], ["unrealKills", "Unreal kills"],
    ["totalTimeCrowdControlDealt", "Crowd control score"], ["timeCCingOthers", "Time crowd controlling others", "high", "time"],
    ["firstBloodKill", "First blood"], ["firstBloodAssist", "First blood assist"], ["longestTimeSpentLiving", "Longest time alive", "high", "time"], ["champLevel", "Champion level"],
  ]],
  ["Damage dealt", [
    ["totalDamageDealtToChampions", "Total damage to champions"], ["physicalDamageDealtToChampions", "Physical damage to champions"],
    ["magicDamageDealtToChampions", "Magic damage to champions"], ["trueDamageDealtToChampions", "True damage to champions"],
    ["totalDamageDealt", "Total damage dealt"], ["physicalDamageDealt", "Physical damage dealt"], ["magicDamageDealt", "Magic damage dealt"], ["trueDamageDealt", "True damage dealt"],
    ["largestCriticalStrike", "Largest critical strike"], ["damageDealtToTurrets", "Damage to turrets"], ["damageDealtToObjectives", "Damage to objectives"],
  ]],
  ["Damage taken & healed", [
    ["totalDamageTaken", "Total damage taken"], ["physicalDamageTaken", "Physical damage taken"], ["magicalDamageTaken", "Magic damage taken"], ["trueDamageTaken", "True damage taken"],
    ["damageSelfMitigated", "Damage self mitigated"], ["totalHeal", "Healing done"], ["totalUnitsHealed", "Units healed"],
    ["totalHealsOnTeammates", "Healing on teammates"], ["totalDamageShieldedOnTeammates", "Shielding on teammates"],
  ]],
  ["Vision", [["visionScore", "Vision score"], ["wardsPlaced", "Wards placed"], ["wardsKilled", "Wards destroyed"],
    ["visionWardsBoughtInGame", "Control wards bought"], ["sightWardsBoughtInGame", "Sight wards bought"]]],
  ["Income", [["goldEarned", "Gold earned"], ["goldSpent", "Gold spent"], ["totalMinionsKilled", "Lane minions killed"],
    ["neutralMinionsKilled", "Jungle monsters killed"], ["neutralMinionsKilledTeamJungle", "Own jungle monsters"], ["neutralMinionsKilledEnemyJungle", "Enemy jungle monsters"], ["totalCs", "Total CS"]]],
  ["Objectives", [["turretKills", "Turret kills"], ["turretTakedowns", "Turret takedowns"], ["inhibitorKills", "Inhibitor kills"], ["inhibitorTakedowns", "Inhibitor takedowns"],
    ["firstTowerKill", "First tower"], ["firstTowerAssist", "First tower assist"], ["firstInhibitorKill", "First inhibitor"], ["firstInhibitorAssist", "First inhibitor assist"]]],
];

export function buildStatGroups(players: Pick<MatchPlayer, "stats">[]): StatGroup[] {
  const keys = new Set(players.flatMap(p => Object.keys(p.stats ?? {})));
  if (keys.has("kills") && keys.has("deaths") && keys.has("assists")) keys.add("kda");
  if (keys.has("totalMinionsKilled") && keys.has("neutralMinionsKilled")) keys.add("totalCs");
  const row = ([key, label, direction = "high", format]: Definition): StatRow => {
    const values = players.map(({ stats: s = {} }): StatValue => {
      const n = (key: string) => typeof s[key] === "number" && Number.isFinite(s[key]) ? s[key] as number : null;
      if (key === "kda") { const k = n("kills"), d = n("deaths"), a = n("assists"); return k == null || d == null || a == null ? null : (k + a) / Math.max(1, d); }
      if (key === "totalCs") { const lane = n("totalMinionsKilled"), jungle = n("neutralMinionsKilled"); return lane == null || jungle == null ? null : lane + jungle; }
      return typeof s[key] === "boolean" ? s[key] : n(key);
    });
    return { key, label, values, leaders: bestValues(values, direction), format };
  };
  const groups = STAT_GROUPS.map(([label, definitions]) => ({ label, rows: definitions.filter(([key]) => keys.delete(key)).map(row) }));
  return groups.filter(group => group.rows.length > 0);
}

export function filterStatGroups(groups: StatGroup[], search: string): StatGroup[] {
  const query = search.trim().toLowerCase();
  if (!query) return groups;
  return groups.map(g => ({ ...g, rows: g.rows.filter(r => `${g.label} ${r.label} ${r.key}`.toLowerCase().includes(query)) })).filter(g => g.rows.length);
}

export type ComparisonRow = { label: string; values: [number | null, number | null] };
export function comparisonRows(players: Pick<MatchPlayer, "teamId" | "kills" | "gold" | "damage" | "vision">[], teams: MatchDetailTeam[]): ComparisonRow[] {
  const rows: ComparisonRow[] = ([ ["Kills", "kills"], ["Gold earned", "gold"], ["Damage to champions", "damage"], ["Vision score", "vision"] ] as const).map(([label, key]) => ({ label,
    values: [100, 200].map(id => { const team = players.filter(p => p.teamId === id); return team.length === 5 && team.every(p => p[key] != null) ? team.reduce((sum, p) => sum + p[key]!, 0) : null; }) as ComparisonRow["values"],
  }));
  for (const label of ["Towers", "Dragons", "Barons", "Heralds", "Grubs", "Inhibitors"]) {
    const values = [100, 200].map(id => teams.find(t => t.teamId === id)?.objectives.find(o => o.label === label)?.kills ?? null) as ComparisonRow["values"];
    if (values.some(v => v != null)) rows.push({ label, values });
  }
  return rows;
}

export type MatchFact = { value: string; label: string; detail: string; teamId: number } & (
  { kind: "gold" | "towers"; values: [number, number] } |
  { kind: "damage"; share: number; damage: number; teamDamage: number } |
  { kind: "vision" }
);
export function matchFacts(players: Pick<MatchPlayer, "teamId" | "me" | "kills" | "gold" | "damage" | "vision">[], teams: MatchDetailTeam[]): MatchFact[] {
  const own = players.find(p => p.me);
  if (!own) return [];
  const rows = comparisonRows(players, teams), index = own.teamId === 100 ? 0 : 1, other = 1 - index;
  const gold = rows.find(r => r.label === "Gold earned")!.values;
  const towers = rows.find(r => r.label === "Towers")?.values;
  const facts: MatchFact[] = [];
  const teamId = own.teamId;
  if (gold[index] != null && gold[other] != null) {
    const diff = gold[index]! - gold[other]!;
    facts.push({kind: "gold", teamId, values: [gold[index]!, gold[other]!], value: `${diff > 0 ? "+" : diff < 0 ? "−" : ""}${Math.abs(diff).toLocaleString("en-US")}`, label: "Gold at the finish", detail: diff === 0 ? "Teams even" : `Your team ${diff > 0 ? "ahead" : "behind"}`});
  }
  if (towers && towers[index] != null && towers[other] != null) facts.push({ kind: "towers", teamId, values: [towers[index]!, towers[other]!], value: `${towers[index]} : ${towers[other]}`, label: "Towers taken", detail: "Your team : enemy" });
  const totalDamage = rows.find(r => r.label === "Damage to champions")!.values[index];
  const share = percent(own.damage, totalDamage);
  if (share != null) facts.push({ kind: "damage", teamId, share, damage: own.damage!, teamDamage: totalDamage!, value: `${Math.round(share)}%`, label: "Your damage share", detail: "Damage to champions" });
  if (facts.length < 3 && own.vision != null) facts.push({kind: "vision", teamId, value: String(own.vision), label: "Your vision score", detail: "Reported by League Client"});
  return facts.slice(0, 3);
}

export function timelineChart(timeline: Timeline) {
  const frames = timeline.frames;
  if (frames.length < 2) return null;
  const start = frames[0].timestamp, duration = frames[frames.length - 1].timestamp - start;
  if (duration <= 0) return null;
  const limit = Math.max(1000, Math.ceil(Math.max(...frames.map(f => Math.abs(f.goldDiff))) / 1000) * 1000);
  const point = (timestamp: number, goldDiff: number) => ({ x: (timestamp - start) / duration * 100, y: 50 - goldDiff / limit * 50 });
  const points = frames.map(f => ({...f, ...point(f.timestamp, f.goldDiff)}));
  const events = timeline.events.filter(e => e.timestamp >= start && e.timestamp <= start + duration).map(event => {
    const right = frames.findIndex(f => f.timestamp >= event.timestamp);
    const b = frames[right], a = frames[Math.max(0, right - 1)];
    const fraction = b.timestamp === a.timestamp ? 0 : (event.timestamp - a.timestamp) / (b.timestamp - a.timestamp);
    return { ...event, ...point(event.timestamp, a.goldDiff + (b.goldDiff - a.goldDiff) * fraction) };
  });
  return { points, events, limit, start, duration };
}

type TimelineParticipant = Pick<MatchPlayer, "participantId" | "teamId" | "championId" | "me">;
export type TimelineKill = TimelineEvent & { kind: "kill"; killer: TimelineParticipant; victim: TimelineParticipant };
export type ResolvedTimelineEvent = TimelineKill | (TimelineEvent & { kind: "objective"; objective: string });
const objectiveLabels: Record<string, string> = { Dragon: "Dragons", Baron: "Barons", Herald: "Heralds", Grub: "Grubs", Tower: "Towers", Inhibitor: "Inhibitors" };

export function resolveTimelineEvents<T extends TimelineEvent>(events: T[], players: TimelineParticipant[]): (T & ResolvedTimelineEvent)[] {
  const participants = new Map(players.filter(p => p.championId > 0).map(p => [p.participantId, p]));
  const result: (T & ResolvedTimelineEvent)[] = [];
  for (const event of events) {
    if (event.killerId != null || event.victimId != null) {
      const killer = participants.get(event.killerId!), victim = participants.get(event.victimId!);
      if (!killer || !victim || killer.teamId === victim.teamId) continue;
      result.push({ ...event, kind: "kill", teamId: killer.teamId, killer, victim });
    } else if (objectiveLabels[event.label] && [100, 200].includes(event.teamId)) {
      result.push({ ...event, kind: "objective", objective: objectiveLabels[event.label] });
    }
  }
  return result.sort((a, b) => a.timestamp - b.timestamp);
}

/** A fight spans at most 30 seconds; later kills start another cluster. */
export function groupKillEvents(events: ResolvedTimelineEvent[]) {
  const clusters: { timestamp: number; events: TimelineKill[] }[] = [];
  for (const event of events) {
    if (event.kind !== "kill") continue;
    const last = clusters[clusters.length - 1];
    if (last && event.timestamp - last.timestamp <= 30000) last.events.push(event);
    else clusters.push({ timestamp: event.timestamp, events: [event] });
  }
  return clusters;
}

export function placementLabel(placement: number | null, win: boolean | null, bestOnTeam: boolean): string {
  if (placement == null || placement < 1) return "–";
  // Each game has one MVP (best on the winning team) and one ACE (best on the losing team), whatever their overall place.
  if (win === true && bestOnTeam) return "MVP";
  if (win === false && bestOnTeam) return "ACE";
  return `${placement}${placement === 1 ? "st" : placement === 2 ? "nd" : placement === 3 ? "rd" : "th"}`;
}

export function teamTotals(players: Pick<MatchDetailPlayer, "kills" | "deaths" | "assists" | "gold" | "damage">[]) {
  const sum = (key: "kills" | "deaths" | "assists" | "gold" | "damage") => players.length === 5 && players.every(p => p[key] != null) ? players.reduce((total, p) => total + p[key]!, 0) : null;
  return { kills: sum("kills"), deaths: sum("deaths"), assists: sum("assists"), gold: sum("gold"), damage: sum("damage") };
}

export function percent(value: number | null, total: number | null): number | null {
  return value != null && total != null && total > 0 ? Math.min(100, value / total * 100) : null;
}

/** Ring colour of a performance score: 70 and up is strong, under 45 is poor. */
export function scoreTone(score: number | null): "high" | "mid" | "low" | "none" {
  if (score == null) return "none";
  return score >= 70 ? "high" : score >= 45 ? "mid" : "low";
}

/** Mean damage of a full team, the tick on each damage bar; null for a partial team. */
export function teamAverageDamage(players: { damage: number | null }[]): number | null {
  return players.length === 5 && players.every(p => p.damage != null) ? players.reduce((total, p) => total + p.damage!, 0) / 5 : null;
}

/** Share of `max` as a 0-100 bar width; 0 for a missing value. */
export const barWidth = (value: number | null, max: number) => value == null || max <= 0 ? 0 : Math.min(100, value / max * 100);
