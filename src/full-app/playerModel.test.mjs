import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readRecents, remember, matchesPrefix, profileGame, profileStats, rankPlayers } from './playerModel.ts';

test('player search ranks exact names before prefixes, merges metadata and caps results', () => {
  const hit = (riotId, lastPlayed = null) => ({ riotId, lastPlayed, icon: null, level: null, ally: null });
  const local = [hit('Example Longer#TAG', 300), { ...hit('Example#TAG', 100), ally: true }];
  const online = [{ ...hit('example#tag'), level: 50, live: true }, hit('Unrelated#TAG')];
  const hits = rankPlayers([...online, ...local], ' EXAMPLE ');
  assert.deepEqual(hits.map(p => p.riotId.toLowerCase()), ['example#tag', 'example longer#tag']);
  assert.equal(hits[0].lastPlayed, 100);
  assert.equal(hits[0].ally, true);
  assert.equal(hits[0].level, 50);
  assert.equal(hits[0].live, true);
  assert.equal(rankPlayers(local, 'example#ta').length, 1);
  assert.equal(rankPlayers(Array.from({ length: 20 }, (_, i) => hit(`Example ${i}#TAG`)), 'example').length, 8);
});

test('recent searches are validated, deduplicated, bounded and reorder on selection', () => {
  let recent = [];
  for (let id = 1; id <= 12; id++) recent = remember(recent, { kind: 'champion', champion: { id, name: `Champion ${id}` } });
  assert.equal(recent.length, 8);
  recent = remember(recent, recent[4]);
  assert.equal(recent[0].champion.id, 8);
  assert.equal(new Set(recent.map(r => r.champion.id)).size, 8);
  assert.deepEqual(readRecents('broken'), []);
  assert.deepEqual(readRecents(' '.repeat(8193)), []);
  assert.deepEqual(readRecents(JSON.stringify([null, { kind: 'page' }, { kind: 'player', riotId: '' } ])), []);
  assert.deepEqual(readRecents(JSON.stringify(recent)), recent);
});

const sample = (id, championId, win, kills = 4, deaths = 2, assists = 6) => ({
  id, championId, win, kills, deaths, assists, createdAt: id * 1000, duration: 1200,
  role: 'adc', metrics: [5, 50, 6, 500, 25, 1], players: [
    { riotId: 'Target#TEST', me: true, ally: true },
    { riotId: id % 2 ? 'Friend#TEST' : 'friend#test', ally: true },
    { riotId: 'Enemy#TEST', ally: false },
    { riotId: 'Once#TEST', ally: id === 1 },
  ],
});

test('profile aggregates top three by games, combined KDA, allies and observed averages', () => {
  const games = [sample(1, 145, true), sample(2, 145, false, 10, 0, 2), sample(3, 236, true), sample(4, 222, false), sample(5, 103, true)];
  const stats = profileStats(games);
  assert.equal(stats.games, 5);
  assert.equal(stats.wins, 3);
  assert.deepEqual(stats.champions.map(c => c.id), [145, 103, 236]);
  assert.equal(stats.champions[0].kda, 11);
  assert.deepEqual([stats.champions[0].wins, stats.champions[0].losses], [1, 1]);
  assert.equal(stats.playedWith.length, 1);
  assert.equal(stats.playedWith[0].games, 5);
  assert.equal(stats.playedWith[0].wins, 3);
  assert.equal(stats.metrics[1].average, 50);
  assert.deepEqual(stats.roles, [{ role: 'adc', games: 5, wins: 3 }]);
});

test('missing metrics stay absent, trends preserve chronology and duplicate games do not inflate a profile', () => {
  const first = { ...sample(1, 145, true), metrics: [null, 40, null, null, null, null], role: '' };
  const last = { ...sample(2, 145, false), metrics: [null, 60, NaN, Infinity, null, null] };
  const stats = profileStats([last, first, first]);
  assert.equal(stats.games, 2);
  assert.deepEqual(stats.metrics[1].points, [40, 60]);
  assert.equal(stats.metrics[1].average, 50);
  assert.equal(stats.metrics[2].average, null);
  assert.equal(stats.metrics[3].average, null);
  assert.equal(profileStats([]).metrics[0].average, null);
  assert.deepEqual(profileStats([]).playedWith, []);
});

test('played-with keeps only the five most frequent teammates', () => {
  const games = Array.from({ length: 14 }, (_, i) => ({ ...sample(i + 1, 145, true),
    players: [{ riotId: 'Target#TEST', me: true, ally: true }, { riotId: `Mate${i % 7}#TEST`, ally: true }] }));
  const stats = profileStats(games);
  assert.equal(stats.playedWith.length, 5);
  assert.ok(stats.playedWith.every(p => p.games === 2));
});

test('played-with icons come from the profile cache without inventing missing icons', () => {
  const stats = profileStats([sample(1, 145, true), sample(2, 145, false)], { 'friend#test': 'data:image/jpeg;base64,AA==' });
  assert.equal(stats.playedWith[0].icon, 'data:image/jpeg;base64,AA==');
  assert.equal(profileStats([sample(1, 145, true), sample(2, 145, false)]).playedWith[0].icon, null);
});

test('a saved profile recomputes the opposing role from the selected player perspective', () => {
  const roles = ['top', 'jungle', 'mid', 'adc', 'support'];
  const players = Array.from({ length: 10 }, (_, i) => ({ riotId: `Player ${i}#TEST`, championId: i + 1, me: i === 0, ally: i < 5, role: roles[i % 5], kills: 2, deaths: 1, assists: 4, cs: 120 }));
  const g = { ...sample(1, 1, true), players, opponent: 5 };
  const result = profileGame(g, 'Player 8#TEST');
  assert.equal(result.role, 'adc');
  assert.equal(result.opponent, 3);
  assert.equal(result.metrics[0], 6);
  assert.equal(result.metrics[1], 60);
});

test('saved player matching is a trimmed, case-insensitive name prefix', () => {
  assert.equal(matchesPrefix('Player One#TEST', ' player'), true);
  assert.equal(matchesPrefix('Player One#TEST', 'one'), false);
  assert.equal(matchesPrefix('Player One#TEST', 'PLAYER ONE#te'), true);
  assert.equal(matchesPrefix('Player One#TEST', ''), false);
});

test('an offline player row uses their perspective without inventing hidden stats', () => {
  const target = { riotId: 'Rival#TEST', championId: 103, me: false, ally: false, kills: 5, deaths: 2, assists: 3, cs: 100, role: 'mid', items: [1] };
  const own = { ...target, riotId: 'You#TEST', me: true, ally: true };
  const game = { id: 1, win: true, duration: 1200, players: [own, target], lpChange: 21, placement: 1, spells: [4], metrics: [1,2,3] };
  const result = profileGame(game, target.riotId);
  assert.equal(result.win, false);
  assert.equal(result.championId, 103);
  assert.equal(result.lpChange, null);
  assert.equal(result.placement, 0);
  assert.equal(result.players[1].me, true);
  assert.equal(result.players[0].ally, false);
  assert.equal(result.metrics[2], 5);
  assert.deepEqual(result.spells, []);
  assert.equal(profileGame(game, 'Missing#TEST'), null);
});
