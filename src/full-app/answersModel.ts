/** Pure Answers logic: threat grouping, search, pool tailoring. No IO. */
import { championId, type Answer } from "./compsData.ts";
import { ROLES, type TeamPlayer } from "./compsModel.ts";

export const ANSWER_TABS = [...ROLES, "draft"] as const;
export type AnswerTab = (typeof ANSWER_TABS)[number];
export type RankedAnswer = Answer & { rank: number };
export type ThreatRow = { key: string; tab: AnswerTab; threat: string[]; kind: Answer["kind"]; answers: RankedAnswer[] };
export type WhoPlays = { player: TeamPlayer; champion: string; entry: NonNullable<TeamPlayer["pool"]>[number] };

const inTab = (a: Answer, tab: AnswerTab) => tab === "draft" ? a.kind === "draft" : a.kind === "lane" && a.role === tab;

/** One row per threat in a tab; answers ranked by games, then win rate. Ranks are fixed before any pool filtering. */
export function groupAnswers(answers: Answer[], tab: AnswerTab): ThreatRow[] {
  const byThreat = new Map<string, ThreatRow>();
  for (const a of answers.filter(e => inTab(e, tab))) {
    const key = `${tab}:${a.threat.join("+")}`;
    if (!byThreat.has(key)) byThreat.set(key, { key, tab, threat: a.threat, kind: a.kind, answers: [] });
    byThreat.get(key)!.answers.push({ ...a, rank: 0 });
  }
  for (const row of byThreat.values()) row.answers.sort((x, y) => y.games - x.games || y.winRate - x.winRate).forEach((a, i) => { a.rank = i + 1; });
  return [...byThreat.values()].sort((x, y) => (y.answers[0]?.games ?? 0) - (x.answers[0]?.games ?? 0));
}

export const matchesQuery = (a: Answer, q: string) => {
  const needle = q.trim().toLowerCase();
  return !needle || [...a.threat, ...a.answer].some(n => n.toLowerCase().includes(needle));
};

/** Players whose pool has any of the answer's champions (role ignored, hidden entries excluded). Active = included. */
export function whoPlays(a: Answer, team: TeamPlayer[]): { active: WhoPlays[]; subs: WhoPlays[] } {
  const active: WhoPlays[] = [], subs: WhoPlays[] = [];
  for (const player of team) for (const champion of a.answer) {
    const entry = (player.pool ?? []).find(e => e.championId === championId(champion) && !e.hidden);
    if (entry) (player.included ? active : subs).push({ player, champion, entry });
  }
  return { active, subs };
}

export const anyonePlays = (a: Answer, team: TeamPlayer[]) => { const w = whoPlays(a, team); return w.active.length + w.subs.length > 0; };

/** Filter rows by search and, when `ours`, by anyone on the roster playing the answer. Ranks stay as ranked. */
export function filterRows(rows: ThreatRow[], q: string, ours: boolean, team: TeamPlayer[]): ThreatRow[] {
  return rows.map(r => ({ ...r, answers: r.answers.filter(a => matchesQuery(a, q) && (!ours || anyonePlays(a, team))) })).filter(r => r.answers.length);
}

/** Row counts per tab for the current search/pool filter, for tab badges and the no-results links. */
export function tabCounts(answers: Answer[], q: string, ours: boolean, team: TeamPlayer[]): Record<AnswerTab, number> {
  return Object.fromEntries(ANSWER_TABS.map(tab => [tab, filterRows(groupAnswers(answers, tab), q, ours, team).length])) as Record<AnswerTab, number>;
}

/** Round-2 filter (13A) for Counters: an answer is playable when anyone on the roster (active or sub) plays it. */
export const answerPlayable = anyonePlays;

/** Split a row's answers into playable and the rest, keeping the fixed ranks and order. */
export function partitionRowAnswers(row: ThreatRow, team: TeamPlayer[]): { playable: RankedAnswer[]; rest: RankedAnswer[] } {
  const playable: RankedAnswer[] = [], rest: RankedAnswer[] = [];
  for (const a of row.answers) (anyonePlays(a, team) ? playable : rest).push(a);
  return { playable, rest };
}

export type RunePageSummary = { primary: NonNullable<Answer["runePages"]>[number]; runnerUp: NonNullable<Answer["runePages"]>[number] | null; totalGames: number };

/** Pro rune pages for the Detail view; null when the answer has none (older data, no mapped games). */
export function runePageSummary(a: Answer): RunePageSummary | null {
  const pages = a.runePages ?? [];
  if (!pages.length) return null;
  const [primary, next] = pages;
  // A runner-up that only swaps a minor rune or shard reads as a duplicate; show it only with a different keystone or trees.
  const distinct = next && (next.runes[0] !== primary.runes[0] || next.primaryStyle !== primary.primaryStyle || next.subStyle !== primary.subStyle);
  return { primary, runnerUp: distinct ? next : null, totalGames: a.games };
}

export function trendOf(a: Answer): { dir: "up" | "down" | "flat"; label: string } {
  if (a.olderMeta) return { dir: "flat", label: "older meta" };
  if (a.trend.length < 2) return { dir: "flat", label: "steady" };
  const d = a.trend[a.trend.length - 1].winRate - a.trend[0].winRate;
  return d > 0.02 ? { dir: "up", label: "rising" } : d < -0.02 ? { dir: "down", label: "falling" } : { dir: "flat", label: "steady" };
}

