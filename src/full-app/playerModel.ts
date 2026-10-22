import type { ChampionOption } from '../runes/types';
import type { Game, Snapshot } from './types';

export type PlayerHit = { riotId: string; icon: string | null; level: number | null; ally: boolean | null; lastPlayed: number | null; live?: boolean };
export type PlayerSearch = { connected: boolean; players: PlayerHit[]; message: string | null };
export type PlayerProfile = { player: PlayerHit; solo: Snapshot['rank']; flex: Snapshot['rank']; games: Game[]; live: boolean; fetchedAt: number; message: string | null };
export type Recent = { kind: 'champion'; champion: ChampionOption } | { kind: 'player'; riotId: string };
export const RECENT_KEY = 'swapper.search.recents.v1';
const key = (item: Recent) => item.kind === 'champion' ? `c:${item.champion.id}` : `p:${item.riotId.toLowerCase()}`;
export function remember(items: Recent[], item: Recent): Recent[] { return [item, ...items.filter(r => key(r) !== key(item))].slice(0, 8); }
export function readRecents(value: string | null): Recent[] {
  if (value && value.length > 8192) return [];
  try {
    const parsed: unknown = JSON.parse(value ?? '[]');
    if (!Array.isArray(parsed)) return [];
    return parsed.slice(0, 64).filter((r): r is Recent => r && (r.kind === 'champion'
      ? Number.isSafeInteger(r.champion?.id) && r.champion.id > 0 && typeof r.champion.name === 'string' && r.champion.name.trim().length > 0 && r.champion.name.length <= 64
      : r.kind === 'player' && typeof r.riotId === 'string' && r.riotId.trim().length > 0 && r.riotId.length <= 128))
      .reduce<Recent[]>((items, item) => items.some(r => key(r) === key(item)) ? items : [...items, item], []).slice(0, 8);
  } catch { return []; }
}
export function matchesPrefix(name: string, query: string) { const q = query.trim().toLowerCase(); return Boolean(q) && name.toLowerCase().startsWith(q); }

/** Saved rows only know a participant's observed stats; leave unobserved fields empty. */
export function profileGame(game: Game, riotId: string): Game | null {
  const player = game.players.find(p => p.riotId.toLowerCase() === riotId.toLowerCase());
  if (!player) return null;
  if (player.me) return game;
  return { ...game, ...player, win: player.ally ? game.win : !game.win, lpChange: null, spells: [], keystone: 0, level: 0, placement: 0,
    metrics: [null, null, game.duration > 0 ? player.cs / (game.duration / 60) : null], opponent: null,
    players: game.players.map(p => ({ ...p, me: p === player, ally: p.ally === player.ally })) };
}
export function savedPlayers(snapshot: Snapshot | null | undefined, query: string): PlayerHit[] {
  const hits = new Map<string, PlayerHit>();
  for (const game of [...snapshot?.games ?? []].sort((a, b) => b.createdAt - a.createdAt)) {
    for (const p of game.players) if (!p.me && matchesPrefix(p.riotId, query) && !hits.has(p.riotId.toLowerCase()))
      hits.set(p.riotId.toLowerCase(), { riotId: p.riotId, icon: null, level: null, ally: p.ally, lastPlayed: game.createdAt });
  }
  return [...hits.values()].slice(0, 8);
}
