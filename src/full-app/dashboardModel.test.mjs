import { test } from "node:test";
import { strict as assert } from "node:assert";
import { defaultQueue, insights, tiles, tierLabel } from "./dashboardModel.ts";

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
  assert.equal(lines[0], "You win 100% (6/6) when you die 5 or fewer times, 0% (0/4) otherwise.");
  assert.ok(lines.length <= 3);
  // Three games on one side are not enough.
  assert.deepEqual(insights(games.slice(0, 9), names).filter(l => l.includes("die")), []);
  // A 10-point gap is not a pattern.
  const even = [
    ...Array.from({ length: 10 }, (_, i) => game({ deaths: 2, win: i < 6 })),
    ...Array.from({ length: 10 }, (_, i) => game({ deaths: 9, win: i < 5 })),
  ];
  assert.deepEqual(insights(even, names).filter(l => l.includes("die")), []);
});

test("tier labels match the benchmark brackets", () => {
  assert.equal(tierLabel("DIAMOND"), "Diamond");
  assert.equal(tierLabel("GRANDMASTER"), "Master+");
  assert.equal(tierLabel(undefined), "all ranks");
});
