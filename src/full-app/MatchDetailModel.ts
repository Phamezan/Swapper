import type { MatchDetailPlayer } from "./types";

export function placementLabel(placement: number | null, win: boolean | null, bestOnTeam: boolean): string {
  if (placement == null || placement < 1) return "–";
  if (placement === 1 && win === true && bestOnTeam) return "MVP";
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
