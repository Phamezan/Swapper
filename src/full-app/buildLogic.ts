import type { ChampionBuild, ChampionCountersView, CounterRow, RuneTree, Selection } from "../runes/types";

/** Match the client's recommended-spell layout: keep Flash on its current key. */
export function spellLayout(recommended: number[], current: number[]): number[] {
  if (recommended.length !== 2) return recommended;
  const target = current.indexOf(4), have = recommended.indexOf(4);
  const flip = target >= 0 && have >= 0 ? target !== have : recommended[0] === current[1] || recommended[1] === current[0];
  return flip ? [recommended[1], recommended[0]] : recommended;
}

/** Both lists use the searched champion's perspective. Deduplicate before sorting. */
export function matchupRows(view: ChampionCountersView | null): CounterRow[] {
  const unique = new Map<number, CounterRow>();
  for (const row of [...(view?.beats ?? []), ...(view?.bestInto ?? [])]) {
    if (!unique.has(row.championId) || unique.get(row.championId)!.games < row.games) unique.set(row.championId, row);
  }
  return [...unique.values()].sort((a, b) => b.delta - a.delta || b.games - a.games || a.name.localeCompare(b.name));
}

/** Indices of strip cards whose art should be mounted: the cards in view plus
 *  `overscan` on each side. Cards are a fixed `step` apart (width + gap). */
export function artWindow(scrollLeft: number, viewport: number, step: number, count: number, overscan = 6): [number, number] {
  if (count <= 0 || step <= 0) return [0, -1];
  const first = Math.max(0, Math.floor(scrollLeft / step) - overscan);
  const last = Math.min(count - 1, Math.ceil((scrollLeft + viewport) / step) + overscan);
  return [first, last];
}

/** The strip order: reliable opponents best to worst, then small-sample ones
 *  (a +60% over 3 games is noise, not the best matchup) in the same order. */
export function stripOrder(rows: CounterRow[]): CounterRow[] {
  return [...rows.filter(row => !row.lowSample), ...rows.filter(row => row.lowSample)];
}

export function overviewSelection(build: ChampionBuild | null | undefined, trees: RuneTree[]): Selection | null {
  const runes = build?.runes;
  if (!runes) return null;
  const primary = trees.find(tree => tree.keystones.some(rune => rune.id === runes.keystone));
  const secondary = trees.find(tree => tree.rows.some(row => row.runes.some(rune => runes.secondaryRunes.includes(rune.id))));
  return primary && secondary ? { ...runes, primaryPageId: primary.id, secondaryPageId: secondary.id } : null;
}

/** Keep reported levels in place; never invent a level order from a max priority. */
export function skillLevels(order: string | null | undefined): { level: number; skill: string }[] {
  return [...(order ?? "").toUpperCase()].map((skill, index) => ({ level: index + 1, skill: "QWER".includes(skill) ? skill : "?" }));
}

export function crestForTier(tier: string | undefined): string {
  const value = (tier ?? "").toLowerCase();
  return ["iron", "bronze", "silver", "gold", "platinum", "emerald", "diamond", "master", "grandmaster", "challenger"].includes(value) ? value : "all";
}
