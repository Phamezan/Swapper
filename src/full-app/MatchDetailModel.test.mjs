import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { barWidth, placementLabel, scoreTone, teamAverageDamage, teamTotals, percent } from './MatchDetailModel.ts';
import { bestValues, buildStatGroups, comparisonRows, filterStatGroups, matchFacts, timelineChart, resolveTimelineEvents, groupKillEvents } from './MatchDetailModel.ts';

test('row leaders share ties, prefer fewer deaths, and never rank unknown or uniform rows', () => {
  assert.deepEqual(bestValues([4, 9, 9, 0], 'high'), [1, 2]);
  assert.deepEqual(bestValues([0, 2, 0, 4], 'low'), [0, 2]);
  assert.deepEqual(bestValues([0, 0, 0], 'high'), []);
  assert.deepEqual(bestValues([3, null, 7], 'high'), []);
  assert.deepEqual(bestValues([false, true, false], 'high'), [1]);
  assert.deepEqual(bestValues([3, 8], 'none'), []);
});

test('named stat groups retain zeroes, omit unlisted fields, and derive CS only from both sources', () => {
  const players = [
    { stats: { kills: 0, deaths: 2, assists: 3, totalMinionsKilled: 80, neutralMinionsKilled: 9, firstBloodKill: false, futureStat: 7 } },
    { stats: { kills: 4, deaths: 0, assists: 1, totalMinionsKilled: 0, firstBloodKill: true } },
  ];
  const groups = buildStatGroups(players);
  const rows = groups.flatMap(g => g.rows);
  assert.deepEqual(rows.find(r => r.key === 'kills').values, [0, 4]);
  assert.deepEqual(rows.find(r => r.key === 'totalCs').values, [89, null]);
  assert.deepEqual(rows.find(r => r.key === 'kda').values, [1.5, 5]);
  assert.ok(!rows.some(r => r.key === 'futureStat'));
  assert.ok(groups.every(g => ['Combat', 'Damage dealt', 'Damage taken & healed', 'Vision', 'Income', 'Objectives'].includes(g.label)));
  assert.deepEqual(buildStatGroups([{stats: {perk0: 8010, item0: 1001, futureStat: 7}}]), []);
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
  assert.deepEqual(facts.map(f => f.kind), ['gold', 'towers', 'damage']);
  assert.deepEqual(facts[0].values, [50000, 45000]);
  assert.deepEqual(facts[1].values, [9, 3]);
  assert.equal(facts[2].share, 20);
  assert.deepEqual(matchFacts(players.filter(p => !p.me), teams), []);
  assert.equal(comparisonRows(players.slice(1), teams)[0].values[0], null);
});

test('hero visuals keep your side first on red and preserve ties, zeroes and missing totals', () => {
  const players = Array.from({length: 10}, (_, i) => ({teamId: i < 5 ? 100 : 200, me: i === 5,
    kills: 0, gold: i < 5 ? 10000 : 9000, damage: i === 5 ? 0 : 100, vision: 0}));
  const teams = [100, 200].map(teamId => ({teamId, objectives: [{label: 'Towers', kills: 0}]}));
  const facts = matchFacts(players, teams);
  assert.equal(facts[0].teamId, 200);
  assert.equal(facts[0].value, '−5,000');
  assert.deepEqual(facts[0].values, [45000, 50000]);
  assert.deepEqual(facts[1].values, [0, 0]);
  assert.equal(facts[2].share, 0);
  assert.equal(facts[2].teamDamage, 400);
  const equal = matchFacts(players.map(p => ({...p, gold: 0, damage: 0})), teams);
  assert.equal(equal[0].value, '0');
  assert.deepEqual(equal[0].values, [0, 0]);
  assert.ok(!equal.some(f => f.kind === 'damage'));
  const partial = matchFacts(players.filter((_, i) => i !== 9), []);
  assert.deepEqual(partial.map(f => [f.kind, f.value]), [['vision', '0']]);
  const missing = matchFacts(players.map((p, i) => i === 6 ? {...p, damage: null} : p), teams);
  assert.ok(!missing.some(f => f.kind === 'damage'));
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

test('timeline resolves kills through participant IDs and skips unknown or allied participants', () => {
  const players = [
    {participantId: 7, teamId: 200, championId: 266, me: true},
    {participantId: 2, teamId: 100, championId: 122, me: false},
    {participantId: 3, teamId: 100, championId: 64, me: false},
  ];
  const event = (timestamp, killerId, victimId) => ({timestamp, teamId: 100, label: 'Champion kill', killerId, victimId});
  const events = resolveTimelineEvents([
    event(1000, 7, 2), event(2000, 3, 7), event(3000, 0, 7), event(4000, 2, 99), event(5000, 2, 3),
    {timestamp: 6000, teamId: 200, label: 'Dragon'},
    {timestamp: 7000, teamId: 100, label: 'Unknown monster'},
  ], players);
  assert.deepEqual(events.map(e => e.kind), ['kill', 'kill', 'objective']);
  assert.equal(events[0].killer.championId, 266);
  assert.equal(events[0].teamId, 200);
  assert.equal(events[0].victim.championId, 122);
  assert.equal(events[1].victim.me, true);
  assert.equal(events[2].objective, 'Dragons');
  const hiddenChampion = resolveTimelineEvents([event(1000, 7, 2)], players.map(p => ({...p, championId: 0})));
  assert.deepEqual(hiddenChampion, []);
});

test('kill clusters group a fight within thirty seconds without chaining the entire match', () => {
  const players = [{participantId: 1, teamId: 100, championId: 266, me: true}, {participantId: 6, teamId: 200, championId: 122, me: false}];
  const events = resolveTimelineEvents([60000, 72000, 90000, 91000, 160000].map(timestamp => ({timestamp, teamId: 100, label: 'Champion kill', killerId: 1, victimId: 6})), players);
  const clusters = groupKillEvents(events);
  assert.deepEqual(clusters.map(c => c.events.length), [3, 1, 1]);
  assert.deepEqual(clusters.map(c => c.timestamp), [60000, 91000, 160000]);
  assert.deepEqual(groupKillEvents([]), []);
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
