import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readRecents, remember, matchesPrefix, profileGame } from './playerModel.ts';

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
