import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { proVariant, mergeProPage, mixPro } from './champ-select-pro.ts';

const match = { matchId: 1, proName: 'Pro A', primaryPageId: 8000, secondaryPageId: 8400, keystone: 8010,
  primaryRunes: [9111, 9104, 8299], secondaryRunes: [8444, 8242], shards: [5005, 5008, 5001], spells: [4, 14],
  completedItems: [{ id: 6692, name: 'Eclipse' }], finalItems: [{ id: 3340, name: 'Ward' }],
  itemPath: [{ minute: 0, items: [{ id: 1055, name: 'Blade', count: 1 }] }], skillOrder: ['Q', 'E', 'W'] };
test('a selected pro supplies every build section without inherited aggregate recommendations', () => {
  const selected = proVariant(match);
  assert.deepEqual(selected.selection.primaryRunes, match.primaryRunes);
  assert.deepEqual(selected.spells, [4, 14]);
  assert.deepEqual(selected.items.items, match.completedItems);
  assert.deepEqual(selected.items.starters, match.itemPath[0].items);
  assert.deepEqual(selected.items.options, []);
  assert.equal(selected.items.coreWinPct, null);
  assert.equal(selected.order, 'QEW');
});
test('missing pro sections stay missing and final trinkets are never imported as core items', () => {
  const selected = proVariant({ ...match, completedItems: [], spells: [], skillOrder: [], itemPath: [] });
  assert.deepEqual(selected.items.items, []);
  assert.deepEqual(selected.spells, []);
  assert.equal(selected.order, null);
  assert.deepEqual(selected.items.starters, []);
});
test('loading another page deduplicates overlapping matches and retains pagination metadata', () => {
  const first = { matches: [{ matchId: 1 }, { matchId: 2 }], page: 1, hasMore: true };
  const next = { matches: [{ matchId: 2 }, { matchId: 3 }], page: 2, hasMore: false };
  assert.deepEqual(mergeProPage(first, next), { ...next, matches: [{ matchId: 1 }, { matchId: 2 }, { matchId: 3 }] });
});
test('boots bought early never take one of the three core slots', () => {
  const completedItems = [{ id: 3171, name: 'Crimson Lucidity' }, { id: 2503, name: 'Blackfire Torch' }, { id: 4629, name: 'Cosmic Drive' }, { id: 3157, name: "Zhonya's Hourglass" }];
  const { items } = proVariant({ ...match, completedItems });
  assert.deepEqual(items.items.slice(0, items.coreLen).map(item => item.id), [2503, 4629, 3157]);
  assert.deepEqual(items.items.at(-1).id, 3171, 'upgraded boots count as boots too');
});
test('a pro build can lend only its runes or only its items to the stats build', () => {
  const base = { label: 'Most played', selection: { keystone: 8005 }, items: { items: [{ id: 1 }] }, spells: [4, 12], order: 'QEW', winPct: 52, games: 900 };
  const pro = proVariant(match);
  const runes = mixPro(base, pro, 'runes');
  assert.equal(runes.selection.keystone, 8010);
  assert.deepEqual([runes.items, runes.spells, runes.order], [base.items, base.spells, base.order]);
  const items = mixPro(base, pro, 'build');
  assert.equal(items.selection.keystone, 8005);
  assert.deepEqual([items.items, items.spells, items.order], [pro.items, pro.spells, pro.order]);
  assert.equal(mixPro(base, pro, 'all'), pro);
});
