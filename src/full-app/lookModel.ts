export const LOOKS = ['glass-context', 'glass-neutral', 'solid-accent'] as const;
export type Look = typeof LOOKS[number];
export const DEFAULT_LOOK: Look = 'glass-context';

export function readLook(search: string, readStored: () => string | null): Look {
  const query = new URLSearchParams(search).get('look');
  if (LOOKS.includes(query as Look)) return query as Look;
  try {
    const stored = readStored();
    if (LOOKS.includes(stored as Look)) return stored as Look;
  } catch { /* A blocked preference must not prevent startup. */ }
  return DEFAULT_LOOK;
}

export function persistLook(look: Look, writeStored: (look: Look) => void): void {
  try { writeStored(look); } catch { /* The live choice still works when storage is blocked. */ }
}

export type AmbientChampions = Record<'champion' | 'player' | 'draft', number | null>;

export function contextualChampion(look: Look, page: string, ids: AmbientChampions, match: number | null): number | null {
  if (look !== 'glass-context') return null;
  if (page.startsWith('Match:')) return match;
  if (page.startsWith('Player:')) return ids.player;
  if (page === 'Champion select') return ids.draft;
  if (page === 'Tier list & Builds' || page === 'Matchups') return ids.champion;
  return null;
}

export function sidebarChampion(primary: number | undefined, alternate: number | undefined, main: number | null): number | null {
  return (primary === main ? alternate : primary) ?? null;
}
