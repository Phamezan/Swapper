import { test } from "node:test";
import { strict as assert } from "node:assert";
import { defaultQueue, deltaText, insights, ladderScore, lpTrend, playTime, rankProgress, sparkline, tiles, tierLabel, today, tone } from "./dashboardModel.ts";

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

test("insights need four games a side and a 20 point gap, strongest first", () => {
  const games = [
    ...Array.from({ length: 6 }, () => game({ deaths: 2, win: true })),
    ...Array.from({ length: 4 }, () => game({ deaths: 9, win: false, cs: 100 })),
  ];
  const lines = insights(games, names);
  assert.deepEqual(lines[0], { topic: "Deaths", condition: "5 or fewer", other: "6 or more", yes: { wins: 6, games: 6, rate: 100 }, no: { wins: 0, games: 4, rate: 0 }, gap: 100 });
  assert.ok(lines.length <= 3);
  assert.ok(lines.every((l, i) => i === 0 || l.gap <= lines[i - 1].gap));
  // Three games on one side are not enough.
  assert.deepEqual(insights(games.slice(0, 9), names).filter(l => l.topic === "Deaths"), []);
  // A 10-point gap is not a pattern.
  const even = [
    ...Array.from({ length: 10 }, (_, i) => game({ deaths: 2, win: i < 6 })),
    ...Array.from({ length: 10 }, (_, i) => game({ deaths: 9, win: i < 5 })),
  ];
  assert.deepEqual(insights(even, names).filter(l => l.topic === "Deaths"), []);
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

test("today sums the games since local midnight and only the LP it knows", () => {
  const now = new Date(2026, 5, 10, 21, 0).getTime();
  const at = (h, extra = {}) => ({ ...game(), createdAt: new Date(2026, 5, 10, h).getTime(), duration: 1800, ...extra });
  const result = today([at(19, { lpChange: 21 }), at(18, { win: false, lpChange: -18 }), at(17, { lpChange: null }), { ...at(0), createdAt: new Date(2026, 5, 9, 23).getTime() }], now);
  assert.deepEqual([result.games, result.wins, result.losses, result.lp, result.seconds], [3, 2, 1, 3, 5400]);
  assert.equal(result.since, new Date(2026, 5, 10, 17).getTime());
  assert.deepEqual(today([], now), { games: 0, wins: 0, losses: 0, lp: null, seconds: 0, since: null });
  assert.equal(today([at(19)], now).lp, null);
  assert.equal(playTime(6720), "1h 52m");
  assert.equal(playTime(2640), "44m");
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
