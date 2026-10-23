import { pageLabel } from './ui/pageNames.ts';

export type LocationSegment = { label: string; page?: string };

/** Navigation targets belong to ancestors; the final segment is the current location. */
export function locationTrail(page: string, context: {
  champion?: string | null;
  match?: { champion: string; win: boolean } | null;
  matchPlayer?: string;
} = {}): LocationSegment[] {
  const player = page.startsWith('Player:') ? page.slice(7) : context.matchPlayer;
  const profile = player ? `Player:${player}` : undefined;
  if (page.startsWith('Match:')) return [
    ...(profile ? [{ label: 'Player', page: profile }, { label: player!, page: profile }]
      : [{ label: 'Dashboard', page: 'Dashboard' }]),
    { label: context.match ? `${context.match.champion} · ${context.match.win ? 'Victory' : 'Defeat'}` : 'Match detail' },
  ];
  if (profile) return [{ label: 'Player', page: profile }, { label: player! }];
  if (context.champion && (page === 'Tier list & Builds' || page === 'Matchups')) return [
    { label: pageLabel(page), page }, { label: context.champion },
  ];
  return [{ label: pageLabel(page) }];
}
