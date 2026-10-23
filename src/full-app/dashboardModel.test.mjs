import { test } from "node:test";
import { strict as assert } from "node:assert";
import { defaultQueue, deltaText, ladderScore, lpTrend, newestFirst, playTime, rankProgress, sparkline, tiles, tierLabel, today, tone } from "./dashboardModel.ts";

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

test("the LP trend needs two recorded points", () => {
  const point = (tier, division, lp, wins, losses) => ({ at: 1, tier, division, lp, wins, losses });
  assert.equal(lpTrend(undefined), null);
  assert.equal(lpTrend([point("GOLD", "II", 10, 1, 1)]), null);
  const trend = lpTrend([point("GOLD", "II", 90, 10, 10), point("GOLD", "I", 12, 11, 10), point("GOLD", "I", 0, 11, 11)]);
  assert.deepEqual(trend, { scores: [1490, 1512, 1500], games: 2 });
  assert.equal(sparkline([1, 3], 100, 20, 0), "0.0,20.0 100.0,0.0");
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
