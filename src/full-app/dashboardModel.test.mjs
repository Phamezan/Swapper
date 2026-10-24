import { test } from "node:test";
import { strict as assert } from "node:assert";
import { defaultQueue, deltaText, ladderScore, lpGraph, newestFirst, smoothPath, playTime, rankProgress, tiles, tierLabel, today, tone } from "./dashboardModel.ts";

const line = (over = {}) => ({ championId: 1, riotId: "", me: false, ally: true, role: "mid", kills: 2, deaths: 2, assists: 2, cs: 150, damage: 10000, gold: 0, vision: 10, items: [], ...over });
function game({ win = true, deaths = 3, cs = 200, oppCs = 150, champion = 103, queue = "solo", opponent = 7, kills = 5, assists = 5 } = {}) {
  const players = [line({ me: true, championId: champion, kills, deaths, assists, cs, damage: 20000 })];
  for (let i = 0; i < 4; i++) players.push(line({ role: ["top", "jungle", "adc", "support"][i] }));
  for (let i = 0; i < 5; i++) players.push(line({ ally: false, role: ["top", "jungle", "mid", "adc", "support"][i], cs: i === 2 ? oppCs : 150 }));
  return { id: Math.random(), queue, championId: champion, createdAt: 0, duration: 1800, win, kills, deaths, assists,
    metrics: [], items: [], spells: [], keystone: 0, cs, level: 18, role: "mid", placement: 0, players, opponent };
}
const names = id => ({ 103: "Ahri", 238: "Zed" })[id] ?? "?";

test("Solo/Duo is the default once five Solo games exist", () => {
  assert.equal(defaultQueue([game(), game(), game(), game()]), "all");
  assert.equal(defaultQueue([game(), game(), game(), game(), game()]), "solo");
});

test("tiles compare with the champion benchmark, else the lane opponent, else nothing", () => {
  const games = [game(), game(), game(), game()];
  const bench = { "103|mid": { kills: 5, deaths: 5, assists: 5, damage: 18000, gold: 0, cs: 180, games: 9000 } };
  const [kda, cs, damage, kp] = tiles(games, bench, "Diamond", names);
  assert.equal(kda.value, 10 / 3);
  assert.deepEqual([kda.versus.label, kda.versus.value], ["Ahri Diamond", 2]);
  assert.equal(cs.versus.delta, 20);
  assert.equal(damage.versus.games, 4);
  // KP has no benchmark: own team kills are 5 + 4*2 = 13, yours 10 → 76.9% vs 4/(5*2)=40%.
  assert.equal(kp.versus.label, "lane opp.");
  assert.equal(Math.round(kp.value), 77);
  // Without benchmarks CS falls back to the lane opponent; with too few games there is no comparison.
  assert.equal(tiles(games, {}, "Diamond", names)[1].versus.label, "lane opp.");
  assert.equal(tiles(games.slice(0, 3), bench, "Diamond", names)[1].versus, null);
  const ambiguous = games.map(g => ({ ...g, opponent: null }));
  assert.equal(tiles(ambiguous, {}, "Diamond", names)[1].versus, null);
});

test("values within 5% of their reference read as even", () => {
  assert.equal(tone(4.17, 2.72), "up");
  assert.equal(tone(2.8, 2.72), "even");
  assert.equal(tone(2.5, 2.72), "down");
});

test("tier labels match the benchmark brackets", () => {
  assert.equal(tierLabel("DIAMOND"), "Diamond");
  assert.equal(tierLabel("GRANDMASTER"), "Master+");
  assert.equal(tierLabel(undefined), "all ranks");
});

const NOW = new Date(2026, 5, 10, 21, 0).getTime();
const hour = h => new Date(2026, 5, 10, h).getTime();
const played = (h, extra = {}) => ({ ...game(), createdAt: hour(h), duration: 1800, ...extra });
const rank = (tier, division, lp) => ({ tier, division, lp, wins: 0, losses: 0 });
const rec = (h, tier, division, lp) => ({ at: hour(h), tier, division, lp, wins: 0, losses: 0 });

test("today counts the games since local midnight", () => {
  const result = today([played(19), played(18, { win: false }), played(17), { ...played(0), createdAt: new Date(2026, 5, 9, 23).getTime() }], NOW);
  assert.deepEqual([result.games, result.wins, result.losses, result.seconds], [3, 2, 1, 5400]);
  assert.equal(result.since, hour(17));
  assert.deepEqual(today([], NOW), { games: 0, wins: 0, losses: 0, lp: null, seconds: 0, since: null });
  assert.equal(playTime(6720), "1h 52m");
  assert.equal(playTime(2640), "44m");
});

test("today's LP is the ladder change since the last record before the first game", () => {
  const games = [played(19, { win: false }), played(18, { win: false }), played(17, { win: false })];
  // 1-3 style day: unrecorded per-game LP no longer matters, only the ladder.
  const history = [rec(9, "GOLD", "II", 50), rec(16, "GOLD", "II", 60), rec(18, "GOLD", "II", 42), rec(20, "GOLD", "II", 30)];
  assert.equal(today(games, NOW, rank("GOLD", "II", 30), history).lp, -30);
  // Promotion Diamond IV 90 -> Diamond III 10, demotion the other way.
  assert.equal(today(games, NOW, rank("DIAMOND", "III", 10), [rec(16, "DIAMOND", "IV", 90)]).lp, 20);
  assert.equal(today(games, NOW, rank("DIAMOND", "IV", 90), [rec(16, "DIAMOND", "III", 10)]).lp, -20);
  // Apex LP is not capped: Master 0 -> Grandmaster 340 is +340.
  assert.equal(today(games, NOW, rank("GRANDMASTER", "", 340), [rec(16, "MASTER", "", 0)]).lp, 340);
  assert.equal(today(games, NOW, rank("MASTER", "", 80), [rec(16, "DIAMOND", "I", 70)]).lp, 110);
  // The live rank wins over a newer-looking record.
  assert.equal(today(games, NOW, rank("GOLD", "II", 70), history).lp, 10);
});

test("today's LP is a dash without a baseline, a rank or a ranked game", () => {
  const solo = [played(19)];
  assert.equal(today(solo, NOW, rank("GOLD", "II", 30), []).lp, null);
  assert.equal(today(solo, NOW, rank("GOLD", "II", 30), [{ ...rec(9, "GOLD", "II", 50), at: hour(18) - 36e5 * 40 }]).lp, -20);
  assert.equal(today(solo, NOW, null, [rec(9, "GOLD", "II", 50)]).lp, null);
  assert.equal(today([played(19, { queue: "normal" })], NOW, rank("GOLD", "II", 30), [rec(9, "GOLD", "II", 50)]).lp, null);
  // Only a record from today, after the first game, still serves as the baseline.
  assert.equal(today(solo, NOW, rank("GOLD", "II", 30), [rec(20, "GOLD", "II", 50)]).lp, -20);
});

test("rank progress counts LP to the next division and skips apex tiers", () => {
  assert.deepEqual(rankProgress({ tier: "EMERALD", division: "II", lp: 64, wins: 0, losses: 0 }), { lp: 64, next: "Emerald I", toNext: 36 });
  assert.equal(rankProgress({ tier: "EMERALD", division: "I", lp: 10, wins: 0, losses: 0 }).next, "Diamond IV");
  assert.equal(rankProgress({ tier: "DIAMOND", division: "I", lp: 10, wins: 0, losses: 0 }).next, "Master");
  assert.deepEqual(rankProgress({ tier: "MASTER", division: "", lp: 120, wins: 0, losses: 0 }), { lp: 120, next: null, toNext: null });
  assert.equal(ladderScore({ tier: "GOLD", division: "I", lp: 90 }), 3 * 400 + 300 + 90);
});

test("the LP graph covers 30 days, frames the line with division guides and sums gains", () => {
  const DAY = 864e5, now = 100 * DAY;
  const point = (daysAgo, tier, division, lp, games = 0) => ({ at: now - daysAgo * DAY, tier, division, lp, wins: games, losses: 0 });
  assert.equal(lpGraph(undefined, now, 100, 50), null);
  assert.equal(lpGraph([point(1, "GOLD", "II", 10)], now, 100, 50), null);
  const history = [point(40, "GOLD", "IV", 0), point(20, "GOLD", "II", 90, 10), point(5, "GOLD", "I", 12, 13), point(0, "PLATINUM", "IV", 40, 16)];
  const graph = lpGraph(history, now, 100, 50);
  // Gold II 90 to Platinum IV 40: lines for the divisions it touched, none for Platinum III above it.
  assert.deepEqual(graph.points.map(p => [p.x, +p.y.toFixed(2), p.games]), [[0, 35, 0], [75, 31.33, 3], [100, 10, 3]]);
  assert.ok(graph.path.startsWith("M0.0,35.0 C") && graph.path.endsWith(" 100.0,10.0"));
  assert.deepEqual(graph.guides.map(g => [g.label, g.tier]), [["II", null], ["I", null], ["IV", "PLATINUM"]]);
  // Emerald I that never reached Diamond: no Diamond crest and no line above Emerald I.
  const below = lpGraph([point(2, "EMERALD", "II", 40), point(1, "EMERALD", "I", 88)], now, 100, 50);
  assert.deepEqual(below.guides.map(g => [g.label, g.tier]), [["II", null], ["I", null]]);
  assert.deepEqual(graph.tiers.map(t => [+t.from.toFixed(2), +t.to.toFixed(2), t.tier]), [[0, 0.33, "PLATINUM"], [0.33, 1, "GOLD"]]);
  assert.equal(graph.month, ladderScore(history[3]) - ladderScore(history[0]), "the 30-day gain starts from the point before the window");
  assert.equal(graph.week, 1640 - 1490);
  assert.equal(graph.peak, history[3]);
  // Only points from the last week: nothing earlier to compare the week with.
  assert.equal(lpGraph(history.slice(2), now, 100, 50).week, 128);
  const apex = lpGraph([point(2, "MASTER", "", 50), point(1, "MASTER", "", 250)], now, 100, 50);
  assert.deepEqual(apex.guides[0], { y: 50, label: "0 LP", tier: "MASTER" });
  assert.deepEqual(apex.tiers, [{ from: 0, to: 1, tier: "MASTER" }]);
  const boundary = lpGraph([point(3, "EMERALD", "IV", 0), point(1, "DIAMOND", "IV", 0)], now, 400, 200);
  assert.ok(boundary.points.every(p => p.y > 0 && p.y < 200), "tier boundaries leave breathing room above and below the series");
  assert.deepEqual(boundary.guides.filter(g => g.tier).map(g => g.tier), ["EMERALD", "DIAMOND"]);
  assert.ok(boundary.tiers.every((t, i, tiers) => t.from < t.to && (!i || t.from === tiers[i - 1].to)), "SVG color stops run from top to bottom without gaps");
  const unordered = [history[3], history[0], history[2], history[1], { ...history[2], lp: 20 }, point(-1, "PLATINUM", "I", 99), point(2, "NONE", "", 0)];
  const original = structuredClone(unordered);
  const sorted = lpGraph(unordered, now, 100, 50);
  assert.deepEqual(sorted.points.map(p => p.point.lp), [90, 20, 40], "sort records, keep the last duplicate, exclude future and unranked points");
  assert.equal(sorted.month, graph.month);
  assert.equal(sorted.week, graph.week);
  assert.deepEqual(unordered, original, "the account history is not mutated");
  assert.equal(lpGraph([history[3], { ...history[3], lp: 30 }], now, 100, 50), null);
  const flat = lpGraph([point(2, "IRON", "IV", 0), point(1, "IRON", "IV", 0)], now, 100, 50);
  assert.ok(flat.points.every(p => Number.isFinite(p.y)));
  assert.equal(flat.month, 0);
});

test("the smooth LP path never overshoots a flat stretch", () => {
  assert.equal(smoothPath([]), "");
  assert.equal(smoothPath([{ x: 0, y: 10 }]), "M0.0,10.0");
  const path = smoothPath([{ x: 0, y: 10 }, { x: 10, y: 10 }, { x: 20, y: 0 }]);
  // The flat first segment keeps both control points on y = 10.
  assert.equal(path.split(" C")[1].split(" ").slice(0, 2).join(" "), "3.3,10.0 6.7,10.0");
  const points = [{ x: 0, y: 80 }, { x: 3, y: 20 }, { x: 90, y: 40 }, { x: 100, y: 0 }];
  smoothPath(points).split(" C").slice(1).forEach((segment, i) => {
    const [a, b, end] = segment.split(" ").map(p => p.split(",").map(Number));
    for (let step = 0; step <= 20; step++) {
      const t = step / 20, s = 1 - t;
      const y = s ** 3 * points[i].y + 3 * s ** 2 * t * a[1] + 3 * s * t ** 2 * b[1] + t ** 3 * end[1];
      assert.ok(y >= Math.min(points[i].y, end[1]) - .1 && y <= Math.max(points[i].y, end[1]) + .1, "irregularly spaced climbs and dips stay inside recorded LP");
    }
  });
});

test("tile deltas read in points for percentages and in percent otherwise", () => {
  const tile = (unit, value, ref, label = "lane opp.") => ({ unit, digits: 0, versus: { label, value: ref, delta: value - ref, games: 5 } });
  assert.equal(deltaText(tile("%", 62, 41)), "+21 pts vs lane opp. 41%");
  assert.equal(deltaText(tile("", 203, 185, "Emerald avg")), "+10% vs Emerald avg 185");
  assert.equal(deltaText(tile("", 150, 200)), "−25% vs lane opp. 200");
  assert.equal(deltaText({ unit: "", digits: 0, versus: null }), null);
});

test("game lists are newest first whatever order they arrive in", () => {
  const games = [{ id: 1, createdAt: 10 }, { id: 3, createdAt: 30 }, { id: 2, createdAt: 20 }];
  assert.deepEqual(newestFirst(games).map(g => g.id), [3, 2, 1]);
  assert.deepEqual(games.map(g => g.id), [1, 3, 2]);
});

test("the LP graph counts no games across a seeded point", () => {
  const DAY = 864e5, now = 100 * DAY;
  const graph = lpGraph([
    { at: now - 3 * DAY, tier: "DIAMOND", division: "IV", lp: 35, wins: 0, losses: 0, source: "opgg" },
    { at: now - DAY, tier: "DIAMOND", division: "III", lp: 10, wins: 40, losses: 30 },
    { at: now, tier: "DIAMOND", division: "III", lp: 38, wins: 41, losses: 30 },
  ], now, 100, 50);
  assert.deepEqual(graph.points.map(p => p.games), [0, 0, 1]);
});
