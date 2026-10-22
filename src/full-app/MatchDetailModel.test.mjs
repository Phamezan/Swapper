import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { barWidth, placementLabel, scoreTone, teamAverageDamage, teamTotals, percent } from './MatchDetailModel.ts';

test('placement labels include MVP, losing-team ACE and correct ordinals', () => {
  assert.equal(placementLabel(1, true, true), 'MVP');
  assert.equal(placementLabel(2, false, true), 'ACE');
  assert.equal(placementLabel(2, null, true), '2nd');
  assert.deepEqual([1, 2, 3, 4, 10, null].map(p => placementLabel(p, true, false)), ['1st', '2nd', '3rd', '4th', '10th', '–']);
});
test('team totals never turn an unknown stat into zero', () => {
  const players = [{ kills: 0, deaths: 2, assists: 4, gold: 1000, damage: 20 }, { kills: 3, deaths: 1, assists: 2, gold: null, damage: 30 }];
  const zeros = Array.from({ length: 3 }, () => ({ kills: 0, deaths: 0, assists: 0, gold: 0, damage: 0 }));
  assert.deepEqual(teamTotals([...players, ...zeros]), { kills: 3, deaths: 3, assists: 6, gold: null, damage: 50 });
  assert.deepEqual(teamTotals(players), { kills: null, deaths: null, assists: null, gold: null, damage: null });
  assert.deepEqual(teamTotals([]), { kills: null, deaths: null, assists: null, gold: null, damage: null });
});
test('ratios guard missing and zero team totals', () => {
  assert.equal(percent(3, 0), null);
  assert.equal(percent(null, 10), null);
  assert.equal(percent(3, 10), 30);
  assert.equal(percent(12, 10), 100);
});
test('score tone, team damage average and bar width never invent values', () => {
  assert.deepEqual([null, 86, 70, 69, 45, 44].map(scoreTone), ['none', 'high', 'high', 'mid', 'mid', 'low']);
  assert.equal(teamAverageDamage([10, 20, 30, 40, 50].map(damage => ({ damage }))), 30);
  assert.equal(teamAverageDamage([10, 20, 30, 40].map(damage => ({ damage }))), null);
  assert.equal(teamAverageDamage([10, 20, 30, 40, null].map(damage => ({ damage }))), null);
  assert.deepEqual([barWidth(50, 200), barWidth(null, 200), barWidth(5, 0), barWidth(300, 200)], [25, 0, 0, 100]);
});
