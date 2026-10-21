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
