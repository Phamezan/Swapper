import type { KeystoneBuildView, ProBuild, ProBuildsView, Selection } from "../runes/types";

export type BuildVariant = { label: string; selection: Selection | null; items: KeystoneBuildView; spells: number[]; order: string | null; winPct: number | null; games: number };

export function proVariant(pro: ProBuild): BuildVariant {
  const items = (pro.completedItems.length ? pro.completedItems : pro.finalItems).filter(item => ![3340, 3363, 3364, 3330].includes(item.id));
  return {
    label: pro.proName,
    selection: { primaryPageId: pro.primaryPageId, secondaryPageId: pro.secondaryPageId, keystone: pro.keystone,
      primaryRunes: pro.primaryRunes, secondaryRunes: pro.secondaryRunes, shards: pro.shards },
    spells: pro.spells, order: pro.skillOrder.length ? pro.skillOrder.join("") : null, winPct: null, games: 1,
    items: { items, starters: pro.itemPath.find(group => group.minute === 0)?.items ?? [], options: [],
      games: 1, coreGames: 1, coreLen: Math.min(3, items.length), coreWinPct: null, stale: false, updatedAt: null, skillPriority: null },
  };
}

export function mergeProPage(previous: ProBuildsView | null, next: ProBuildsView): ProBuildsView {
  const matches = new Map((previous?.matches ?? []).map(match => [match.matchId, match]));
  for (const match of next.matches) matches.set(match.matchId, match);
  return { ...next, matches: [...matches.values()] };
}
