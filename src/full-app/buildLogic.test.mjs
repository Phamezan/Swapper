import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { matchupRows, stripRows, MIN_STRIP_GAMES, artWindow, stripOrder, skillLevels, crestForTier, overviewSelection, spellLayout } from './buildLogic.ts';

test('a chosen Flash key wins; keep follows the current layout', () => {
  assert.deepEqual(spellLayout([14, 4], [12, 4], 'd'), [4, 14]);
  assert.deepEqual(spellLayout([4, 14], [4, 12], 'f'), [14, 4]);
  assert.deepEqual(spellLayout([4, 14], [12, 4], 'keep'), [14, 4]);
  assert.deepEqual(spellLayout([12, 14], [14, 12], 'd'), [14, 12], 'no Flash: the current order still wins');
});

test('matchups retain the player perspective, deduplicate by sample, and sort best to worst', () => {
  const row = (championId, delta, games) => ({ championId, delta, games, name: String(championId) });
  assert.deepEqual(matchupRows({ beats: [row(1, 3, 400), row(2, 1, 100)], bestInto: [row(3, -4, 500), row(2, 2, 200)] }).map(row => [row.championId, row.delta]), [[1, 3], [2, 2], [3, -4]]);
  assert.deepEqual(matchupRows(null), []);
});
test('missing and unknown skill levels never turn into fabricated recommendations', () => {
  assert.deepEqual(skillLevels(null), []);
  assert.deepEqual(skillLevels('q?er'), [{ level: 1, skill: 'Q' }, { level: 2, skill: '?' }, { level: 3, skill: 'E' }, { level: 4, skill: 'R' }]);
});
test('every League tier has its own crest including lower ranks and Grandmaster', () => {
  for (const tier of ['IRON', 'BRONZE', 'SILVER', 'GOLD', 'PLATINUM', 'EMERALD', 'DIAMOND', 'MASTER', 'GRANDMASTER', 'CHALLENGER']) assert.equal(crestForTier(tier), tier.toLowerCase());
  assert.equal(crestForTier('unranked'), 'all');
});
test('a rune page can only be applied after both catalog trees are resolved', () => {
  const build = { runes: { keystone: 1, primaryRunes: [2], secondaryRunes: [3], shards: [4] } };
  assert.equal(overviewSelection(build, []), null);
  assert.deepEqual(overviewSelection(build, [{ id: 100, keystones: [{ id: 1 }], rows: [{ runes: [{ id: 2 }] }] }, { id: 200, keystones: [], rows: [{ runes: [{ id: 3 }] }] }]), { ...build.runes, primaryPageId: 100, secondaryPageId: 200 });
});
test('strip art mounts only near the visible scroll window', () => {
  assert.deepEqual(artWindow(0, 600, 120, 170, 2), [0, 7]);
  assert.deepEqual(artWindow(1200, 600, 120, 170, 2), [8, 17]);
  assert.deepEqual(artWindow(20000, 600, 120, 170, 2), [164, 169]);
  assert.deepEqual(artWindow(0, 600, 120, 0), [0, -1]);
});
test('the strip puts small samples after every reliable matchup', () => {
  const row = (championId, delta, lowSample = false) => ({ championId, delta, games: 100, lowSample, name: String(championId) });
  assert.deepEqual(stripOrder([row(1, 60, true), row(2, 3), row(3, -2), row(4, -40, true)]).map(r => r.championId), [2, 3, 1, 4]);
});

test('the matchup strip keeps opponents with 300+ games across both patches, best or toughest first', () => {
  const row = (championId, delta, games, prevGames = 0) => ({ championId, name: `C${championId}`, winPct: 50 + delta, delta, games, prevGames, lowSample: false });
  const view = { beats: [row(1, 5, 1000), row(2, 76, 3), row(3, -4, 20, 400), row(4, 1, 299)], bestInto: [] };
  assert.equal(MIN_STRIP_GAMES, 300);
  assert.deepEqual(stripRows(view).map(r => r.championId), [1, 3]);
  assert.deepEqual(stripRows(view, true).map(r => r.championId), [3, 1]);
  assert.deepEqual(stripRows(null), []);
});
