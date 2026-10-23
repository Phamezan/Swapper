import { test } from 'node:test';
import assert from 'node:assert/strict';
import { contextualChampion, sidebarChampion } from './lookModel.ts';

test('only contextual pages use their champion', () => {
  const ids = { champion: 266, player: 64, draft: 103 };
  for (const page of ['Dashboard', 'Tierlist', 'Accounts', 'Settings', 'Live Game']) {
    assert.equal(contextualChampion(page, ids, null), null);
  }
  assert.equal(contextualChampion('Tier list & Builds', ids, null), 266);
  assert.equal(contextualChampion('Matchups', ids, null), 266);
  assert.equal(contextualChampion('Player:Player 1#DEMO', ids, null), 64);
  assert.equal(contextualChampion('Champion select', ids, null), 103);
  assert.equal(contextualChampion('Match:10', ids, 122), 122);
});

test('sidebar avoids repeating the main art and falls back without inventing a champion', () => {
  assert.equal(sidebarChampion(266, 122, 266), 122);
  assert.equal(sidebarChampion(266, undefined, 266), null);
  assert.equal(sidebarChampion(266, 122, 64), 266);
});
