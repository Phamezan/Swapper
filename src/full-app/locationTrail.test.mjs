import { test } from 'node:test';
import assert from 'node:assert/strict';
import { locationTrail } from './locationTrail.ts';

test('top-level locations use the app page labels', () => {
  assert.deepEqual(locationTrail('Dashboard'), [{ label: 'Dashboard' }]);
  assert.deepEqual(locationTrail('Champion select'), [{ label: 'Draft' }]);
  assert.deepEqual(locationTrail('Live Game'), [{ label: 'Live game' }]);
});

test('champion locations link back to their own tools page', () => {
  assert.deepEqual(locationTrail('Tier list & Builds', { champion: 'Aatrox' }), [
    { label: 'Tierlist', page: 'Tier list & Builds' }, { label: 'Aatrox' },
  ]);
  assert.deepEqual(locationTrail('Matchups', { champion: 'Ahri' }), [
    { label: 'Matchups', page: 'Matchups' }, { label: 'Ahri' },
  ]);
  assert.deepEqual(locationTrail('Dashboard', { champion: 'Ahri' }), [{ label: 'Dashboard' }]);
});

test('dashboard matches show their champion and outcome, including defeat', () => {
  for (const win of [true, false]) assert.deepEqual(locationTrail('Match:42', { match: { champion: 'Aatrox', win } }), [
    { label: 'Dashboard', page: 'Dashboard' }, { label: `Aatrox · ${win ? 'Victory' : 'Defeat'}` },
  ]);
  assert.deepEqual(locationTrail('Match:42'), [{ label: 'Dashboard', page: 'Dashboard' }, { label: 'Match detail' }]);
});

test('player and player-match ancestors link to the actual profile, independent of history', () => {
  const riotId = 'Player: With spaces#DEMO';
  const profile = `Player:${riotId}`;
  assert.deepEqual(locationTrail(profile), [{ label: 'Player', page: profile }, { label: riotId }]);
  assert.deepEqual(locationTrail('Match:42:encoded', { matchPlayer: riotId, match: { champion: 'Ahri', win: true } }), [
    { label: 'Player', page: profile }, { label: riotId, page: profile }, { label: 'Ahri · Victory' },
  ]);
});
