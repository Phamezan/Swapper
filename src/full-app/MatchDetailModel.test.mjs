import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { barWidth, placementLabel, scoreTone, teamAverageDamage, teamTotals, percent } from './MatchDetailModel.ts';
import { bestValues, buildStatGroups, comparisonRows, filterStatGroups, matchFacts, timelineChart } from './MatchDetailModel.ts';

test('row leaders share ties, prefer fewer deaths, and never rank unknown or uniform rows', () => {
  assert.deepEqual(bestValues([4, 9, 9, 0], 'high'), [1, 2]);
  assert.deepEqual(bestValues([0, 2, 0, 4], 'low'), [0, 2]);
  assert.deepEqual(bestValues([0, 0, 0], 'high'), []);
  assert.deepEqual(bestValues([3, null, 7], 'high'), []);
  assert.deepEqual(bestValues([false, true, false], 'high'), [1]);
  assert.deepEqual(bestValues([3, 8], 'none'), []);
});

test('stat groups retain observed zeroes and new fields, derive CS only from both sources, and filter labels and raw keys', () => {
  const players = [
    { stats: { kills: 0, deaths: 2, assists: 3, totalMinionsKilled: 80, neutralMinionsKilled: 9, firstBloodKill: false, futureStat: 7 } },
    { stats: { kills: 4, deaths: 0, assists: 1, totalMinionsKilled: 0, firstBloodKill: true } },
  ];
  const groups = buildStatGroups(players);
  const rows = groups.flatMap(g => g.rows);
  assert.deepEqual(rows.find(r => r.key === 'kills').values, [0, 4]);
  assert.deepEqual(rows.find(r => r.key === 'totalCs').values, [89, null]);
  assert.deepEqual(rows.find(r => r.key === 'kda').values, [1.5, 5]);
  assert.deepEqual(rows.find(r => r.key === 'futureStat').values, [7, null]);
  assert.ok(!rows.some(r => r.key === 'totalDamageShieldedOnTeammates'));
  assert.equal(filterStatGroups(groups, 'first blood')[0].rows.length, 1);
  assert.equal(filterStatGroups(groups, 'neutralMinionsKilled')[0].rows.length, 1);
  assert.deepEqual(filterStatGroups(groups, 'absent'), []);
});

test('comparison and hero facts use full teams and distinguish final totals from a timeline lead', () => {
  const players = Array.from({length: 10}, (_, i) => ({ participantId: i + 1, teamId: i < 5 ? 100 : 200, me: i === 0,
    kills: i < 5 ? 4 : 2, deaths: 2, assists: 3, gold: i < 5 ? 10000 : 9000, damage: 20000, vision: 20 }));
  const teams = [100, 200].map(teamId => ({teamId, objectives: [{label: 'Towers', kills: teamId === 100 ? 9 : 3}]}));
  assert.deepEqual(comparisonRows(players, teams).find(r => r.label === 'Gold earned').values, [50000, 45000]);
  const facts = matchFacts(players, teams);
  assert.equal(facts[0].value, '+5,000');
  assert.match(facts[0].label, /gold at the finish/i);
  assert.equal(facts[1].value, '9 : 3');
  assert.equal(facts[2].value, '20%');
  assert.deepEqual(matchFacts(players.filter(p => !p.me), teams), []);
  assert.equal(comparisonRows(players.slice(1), teams)[0].values[0], null);
});

test('timeline geometry preserves time spacing, signed leads and event placement without inventing extra samples', () => {
  const data = { frames: [{timestamp: 0, goldDiff: 0}, {timestamp: 60000, goldDiff: -2000}, {timestamp: 180000, goldDiff: 4000}],
    events: [{timestamp: 120000, teamId: 200, label: 'Dragon'}] };
  const chart = timelineChart(data);
  assert.equal(chart.points.length, 3);
  assert.ok(Math.abs(chart.points[1].x - 100 / 3) < 1e-8);
  assert.equal(chart.points[1].y, 75);
  assert.equal(chart.events[0].y, 37.5);
  assert.equal(chart.limit, 4000);
  assert.equal(timelineChart({frames: [], events: []}), null);
});

test('placement labels include MVP, losing-team ACE and correct ordinals', () => {
  assert.equal(placementLabel(1, true, true), 'MVP');
  assert.equal(placementLabel(2, false, true), 'ACE');
  assert.equal(placementLabel(2, true, true), 'MVP', "the winning team's best is MVP even when a loser scored higher");
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
