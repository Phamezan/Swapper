export type AmbientChampions = Record<'champion' | 'player' | 'draft', number | null>;

/** The champion whose art backs the page, or null for the neutral client art. */
export function contextualChampion(page: string, ids: AmbientChampions, match: number | null): number | null {
  if (page.startsWith('Match:')) return match;
  if (page.startsWith('Player:')) return ids.player;
  if (page === 'Champion select') return ids.draft;
  if (page === 'Tier list & Builds' || page === 'Matchups') return ids.champion;
  return null;
}

export function sidebarChampion(primary: number | undefined, alternate: number | undefined, main: number | null): number | null {
  return (primary === main ? alternate : primary) ?? null;
}
