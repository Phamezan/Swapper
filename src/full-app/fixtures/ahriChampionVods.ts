import type { MatchupVod, MomentKind } from "../matchupGuide";
import { ahriVsZedVods } from "./ahriVsZedVods.ts";

// Design preview only (dev builds): invented players, unplayable IDs. Sized like a popular champion's library.
const GENERATED = 1200;
const ENEMIES = [[238, "Zed", "mid", 6], [157, "Yasuo", "mid", 4], [7, "LeBlanc", "mid", 3], [4, "Twisted Fate", "mid", 3], [134, "Syndra", "mid", 3],
  [61, "Orianna", "mid", 2], [245, "Ekko", "mid", 2], [91, "Talon", "mid", 2], [112, "Viktor", "mid", 2], [517, "Sylas", "mid", 2], [84, "Akali", "mid", 2],
  [1, "Annie", "mid", 1], [13, "Ryze", "mid", 1], [105, "Fizz", "mid", 1], [777, "Yone", "mid", 1], [163, "Taliyah", "mid", 1],
  [412, "Thresh", "support", 1], [89, "Leona", "support", 1], [266, "Aatrox", "top", 1]] as const;
const RANKS = ["Challenger", "KR Challenger", "Grandmaster", "KR Grandmaster", "Grand Master", "MASTER", "Master", "EUW Diamond 1", "Diamond"];
const TEAMS = ["T1", "Gen.G", "G2 Esports", "HLE", "KT", "Fnatic", "BLG", "DK"];
const PATCHES = ["26.21", "26.21", "26.20", "26.20", "26.19", "26.18", "26.10", "26.9"];
const KINDS: MomentKind[] = ["kill", "death", "teamfight", "dragon", "herald", "tower", "roam", "trade"];
const LABELS = ["Ahri takes down", "Caught by", "Wins the skirmish against", "Trades into", "Roams past"];

let seed = 103;
// MINSTD stays within exact integers, so the preview list is identical every run.
const pick = (n: number) => (seed = seed * 48271 % 2147483647) % n;
const pool = ENEMIES.flatMap(enemy => Array<typeof enemy>(enemy[3]).fill(enemy));

function generated(index: number): MatchupVod {
  const [enemy, name, lane] = pool[pick(pool.length)];
  const team = pick(3) === 0 ? TEAMS[pick(TEAMS.length)] : undefined;
  const player = (team || pick(2)) ? `Fixture ${team ? "Pro" : "Solo"} ${index + 1}` : undefined;
  const durationSec = 1080 + pick(1500);
  const moments = Array.from({ length: pick(7) }, (_, i) => {
    const kind = KINDS[pick(KINDS.length)];
    return { atSec: Math.floor(durationSec * (i + 1) / 8), label: `${LABELS[pick(LABELS.length)]} ${name}`, kind };
  });
  const result = (["W", "L", undefined] as const)[pick(3)];
  return { videoId: `fixture-ahri-${index + 6}`, fixture: true, enemy, lane, title: `Ahri vs ${name} · ${lane === "mid" ? "mid lane" : lane} replay ${index + 1}`,
    player, team, rankClaim: RANKS[pick(RANKS.length)], patch: PATCHES[pick(PATCHES.length)], result, durationSec, moments };
}

export const ahriChampionVods: MatchupVod[] = [
  ...ahriVsZedVods.map(vod => ({ ...vod, enemy: 238, lane: "mid" as const })),
  ...Array.from({ length: GENERATED }, (_, index) => generated(index)),
];
