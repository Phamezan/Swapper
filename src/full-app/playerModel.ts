import type { ChampionOption } from '../runes/types';
import type { Game, Snapshot } from './types';
import type { LiveGameView } from './LiveGameModel';

export type PlayerHit = { riotId: string; icon: string | null; level: number | null; ally: boolean | null; lastPlayed: number | null; live?: boolean };
export type PlayerSearch = { connected: boolean; players: PlayerHit[]; message: string | null };
export type PlayerLive = { live: boolean; view: LiveGameView | null; message: string | null };
export type ProfileRank = NonNullable<Snapshot['rank']> & { recordAvailable?: boolean; recordSource?: 'ugg' };
export type PlayerProfile = { player: PlayerHit; region?: string | null; solo: ProfileRank | null; flex: Snapshot['rank']; profileIcons?: Record<string, string>; games: Game[]; live: boolean; source?: 'ugg' | null; fetchedAt: number; message: string | null };
export type SeasonPage = { games: Game[]; done: boolean };
/** u.gg pages bound for one season; matches the backend's limit. */
export const SEASON_PAGES = 100;
/** A season has dozens of repeat teammates; the side column shows the most frequent. */
const PLAYED_WITH_LIMIT = 5;
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

export function rankPlayers(players: PlayerHit[], query: string): PlayerHit[] {
  const q = query.trim().toLowerCase();
  const hits = new Map<string, PlayerHit>();
  for (const player of players) {
    if (!matchesPrefix(player.riotId, q)) continue;
    const key = player.riotId.toLowerCase(), previous = hits.get(key);
    hits.set(key, previous ? { ...previous, icon: previous.icon ?? player.icon, level: previous.level ?? player.level,
      ally: (player.lastPlayed ?? 0) > (previous.lastPlayed ?? 0) ? player.ally : previous.ally ?? player.ally,
      lastPlayed: Math.max(previous.lastPlayed ?? 0, player.lastPlayed ?? 0) || null, live: previous.live || player.live } : player);
  }
  const exact = (id: string) => id.toLowerCase() === q || id.split('#')[0].toLowerCase() === q;
  return [...hits.values()].sort((a, b) => Number(exact(b.riotId)) - Number(exact(a.riotId))
    || (b.lastPlayed ?? 0) - (a.lastPlayed ?? 0) || a.riotId.toLowerCase().localeCompare(b.riotId.toLowerCase())).slice(0, 8);
}

/** Saved rows only know a participant's observed stats; leave unobserved fields empty. */
export function profileGame(game: Game, riotId: string): Game | null {
  const player = game.players.find(p => p.riotId.toLowerCase() === riotId.toLowerCase());
  if (!player) return null;
  if (player.me) return game;
  const players = game.players.map(p => ({ ...p, me: p === player, ally: p.ally === player.ally }));
  const allies = players.filter(p => p.ally);
  const totalKills = allies.reduce((total, p) => total + p.kills, 0);
  const opposing = players.map((p, i) => ({ ...p, index: i })).filter(p => !p.ally && p.role === player.role);
  const opponent = players.length === 10 && allies.length === 5 && ['top', 'jungle', 'mid', 'adc', 'support'].includes(player.role)
    && allies.filter(p => p.role === player.role).length === 1 && opposing.length === 1 ? opposing[0].index : null;
  return { ...game, ...player, win: player.ally ? game.win : !game.win, lpChange: null, spells: [], keystone: 0, level: 0, placement: 0,
    metrics: [(player.kills + player.assists) / Math.max(1, player.deaths), allies.length === 5 && totalKills > 0 ? Math.min(100, 100 * (player.kills + player.assists) / totalKills) : null,
      game.duration > 0 ? player.cs / (game.duration / 60) : null, null, null, null], opponent, players };
}

export function profileStats(loaded: Game[], profileIcons: Record<string, string> = {}) {
  const games = [...new Map(loaded.map(game => [game.id, game])).values()].sort((a, b) => a.createdAt - b.createdAt);
  const champions = new Map<number, { id: number; games: number; wins: number; losses: number; kills: number; deaths: number; assists: number }>();
  const together = new Map<string, { riotId: string; games: number; wins: number }>();
  const roles = new Map<string, { role: string; games: number; wins: number }>();
  for (const game of games) {
    const champion = champions.get(game.championId) ?? { id: game.championId, games: 0, wins: 0, losses: 0, kills: 0, deaths: 0, assists: 0 };
    champion.games++; champion.wins += Number(game.win); champion.losses += Number(!game.win);
    champion.kills += game.kills; champion.deaths += game.deaths; champion.assists += game.assists;
    champions.set(champion.id, champion);
    const seen = new Set<string>();
    for (const player of game.players) {
      const key = player.riotId.trim().toLowerCase();
      if (player.me || !player.ally || !key || seen.has(key)) continue;
      seen.add(key);
      const ally = together.get(key) ?? { riotId: player.riotId, games: 0, wins: 0 };
      ally.games++; ally.wins += Number(game.win); together.set(key, ally);
    }
    if (['top', 'jungle', 'mid', 'adc', 'support'].includes(game.role)) {
      const role = roles.get(game.role) ?? { role: game.role, games: 0, wins: 0 };
      role.games++; role.wins += Number(game.win); roles.set(role.role, role);
    }
  }
  return {
    games: games.length, wins: games.filter(g => g.win).length,
    champions: [...champions.values()].sort((a, b) => b.games - a.games || b.wins - a.wins || a.id - b.id).slice(0, 3)
      .map(c => ({ ...c, kda: (c.kills + c.assists) / Math.max(1, c.deaths) })),
    playedWith: [...together.values()].filter(p => p.games >= 2).sort((a, b) => b.games - a.games || b.wins - a.wins || a.riotId.localeCompare(b.riotId)).slice(0, PLAYED_WITH_LIMIT)
      .map(p => ({ ...p, icon: profileIcons[p.riotId.toLowerCase()] ?? null })),
    roles: [...roles.values()].sort((a, b) => b.games - a.games || a.role.localeCompare(b.role)),
    metrics: Array.from({ length: 6 }, (_, index) => {
      const points = games.map(game => game.metrics[index]).filter((v): v is number => v != null && Number.isFinite(v));
      const average = points.length ? points.reduce((a, b) => a + b, 0) / points.length : null;
      const recent = games.slice(-5).map(game => game.metrics[index]).filter((v): v is number => v != null && Number.isFinite(v));
      return { average, points, samples: points.length, delta: average != null && recent.length ? recent.reduce((a, b) => a + b, 0) / recent.length - average : null };
    }),
  };
}
export function savedPlayers(snapshot: Snapshot | null | undefined, query: string): PlayerHit[] {
  const hits = new Map<string, PlayerHit>();
  for (const game of [...snapshot?.games ?? []].sort((a, b) => b.createdAt - a.createdAt)) {
    for (const p of game.players) if (!p.me && matchesPrefix(p.riotId, query) && !hits.has(p.riotId.toLowerCase()))
      hits.set(p.riotId.toLowerCase(), { riotId: p.riotId, icon: null, level: null, ally: p.ally, lastPlayed: game.createdAt });
  }
  return rankPlayers([...hits.values()], query);
}
