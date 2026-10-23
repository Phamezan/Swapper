import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readLook, persistLook, contextualChampion, sidebarChampion } from './lookModel.ts';

test('look query overrides the saved preference in every build; unknown values and blocked storage are safe', () => {
  assert.equal(readLook('?look=glass-neutral', () => 'solid-accent'), 'glass-neutral');
  assert.equal(readLook('?look=unknown', () => 'solid-accent'), 'solid-accent');
  assert.equal(readLook('', () => { throw new Error('Storage blocked'); }), 'glass-context');
  assert.equal(readLook('?look=constructor', () => '../invalid'), 'glass-context');
});

test('a look choice persists and blocked storage does not prevent the live change', () => {
  let saved = null;
  persistLook('glass-neutral', value => { saved = value; });
  assert.equal(readLook('', () => saved), 'glass-neutral');
  assert.doesNotThrow(() => persistLook('solid-accent', () => { throw new Error('Storage blocked'); }));
});

test('only contextual pages use their champion, and neutral/solid never do', () => {
  const ids = { champion: 266, player: 64, draft: 103 };
  for (const page of ['Dashboard', 'Tierlist', 'Accounts', 'Settings', 'Live Game']) {
    assert.equal(contextualChampion('glass-context', page, ids, null), null);
  }
  assert.equal(contextualChampion('glass-context', 'Tier list & Builds', ids, null), 266);
  assert.equal(contextualChampion('glass-context', 'Matchups', ids, null), 266);
  assert.equal(contextualChampion('glass-context', 'Player:Player 1#DEMO', ids, null), 64);
  assert.equal(contextualChampion('glass-context', 'Champion select', ids, null), 103);
  assert.equal(contextualChampion('glass-context', 'Match:10', ids, 122), 122);
  for (const look of ['glass-neutral', 'solid-accent']) {
    assert.equal(contextualChampion(look, 'Match:10', ids, 122), null);
  }
});

test('sidebar avoids repeating the main art and falls back without inventing a champion', () => {
  assert.equal(sidebarChampion(266, 122, 266), 122);
  assert.equal(sidebarChampion(266, undefined, 266), null);
  assert.equal(sidebarChampion(266, 122, 64), 266);
});
