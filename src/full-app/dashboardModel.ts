// Pure Dashboard logic: queue default and the five comparison tiles.
// Kept free of React so node:test can cover it.
import { fmtCompact } from "./ui/format.ts";
import type { Bench, Game, LpPoint, Player, Snapshot } from "./types";

/** Fewer comparable games than this and a tile shows only the user's value. */
export const MIN_COMPARED = 4;

export const benchKey = (game: Game) => `${game.championId}|${game.role}`;

/** Games newest first, whatever order the snapshot arrived in. */
export const newestFirst = (games: Game[]) => [...games].sort((a, b) => b.createdAt - a.createdAt);

export function defaultQueue(games: Game[]) {
  return games.filter(g => g.queue === "solo").length >= 5 ? "solo" : "all";
}

export const me = (game: Game) => game.players.find(p => p.me);
export const opponent = (game: Game) => game.opponent == null ? undefined : game.players[game.opponent];

const sum = (values: number[]) => values.reduce((a, b) => a + b, 0);
const mean = (values: number[]) => values.length ? sum(values) / values.length : null;

/** Kill participation and damage share of `player` within their team. */
function teamShare(game: Game, player: Player, field: "kp" | "damage") {
  const team = game.players.filter(p => p.ally === player.ally);
  if (team.length !== 5) return null;
  if (field === "kp") {
    const kills = sum(team.map(p => p.kills));
    return kills ? Math.min(100, 100 * (player.kills + player.assists) / kills) : null;
  }
  const damage = sum(team.map(p => p.damage));
  return damage ? 100 * player.damage / damage : null;
}

type Stat = { label: string; unit: string; digits: number; of: (game: Game, p: Player) => number | null; bench?: (b: Bench) => number };
const kda = (k: number, d: number, a: number) => (k + a) / Math.max(1, d);
const STATS: Record<string, Stat> = {
  kda: { label: "KDA", unit: "", digits: 2, of: (_, p) => kda(p.kills, p.deaths, p.assists), bench: b => kda(b.kills, b.deaths, b.assists) },
  cs: { label: "CS / game", unit: "", digits: 0, of: (_, p) => p.cs, bench: b => b.cs },
  damage: { label: "Damage / game", unit: "", digits: 0, of: (_, p) => p.damage, bench: b => b.damage },
  kp: { label: "Kill participation", unit: "%", digits: 0, of: (g, p) => teamShare(g, p, "kp") },
  share: { label: "Damage share", unit: "%", digits: 0, of: (g, p) => teamShare(g, p, "damage") },
  vision: { label: "Vision / min", unit: "", digits: 2, of: (g, p) => p.vision / Math.max(1, g.duration / 60) },
};

export type Tile = {
  key: string; label: string; unit: string; digits: number;
  value: number | null;
  /** What the value is compared with, when there were enough games. */
  versus: { label: string; value: number; delta: number; games: number } | null;
};

export function mainRole(games: Game[]) {
  const counts = new Map<string, number>();
  for (const g of games) counts.set(g.role, (counts.get(g.role) ?? 0) + 1);
  return [...counts].sort((a, b) => b[1] - a[1])[0]?.[0] ?? "mid";
}

/** KDA, CS and damage against the same champion in the user's tier (falling
 *  back to the lane opponent), KP and damage share against the lane opponent.
 *  Supports get vision instead of CS. */
export function tiles(games: Game[], benches: Record<string, Bench>, tierLabel: string, championName: (id: number) => string): Tile[] {
  const keys = ["kda", mainRole(games) === "support" ? "vision" : "cs", "damage", "kp", "share"];
  const champions = new Set(games.map(g => g.championId));
  const benchLabel = champions.size === 1 ? `${championName([...champions][0])} ${tierLabel}` : `${tierLabel} avg`;
  return keys.map(key => {
    const stat = STATS[key];
    const own = games.flatMap(g => { const p = me(g); const v = p && stat.of(g, p); return v == null ? [] : [v]; });
    const tile: Tile = { key, label: stat.label, unit: stat.unit, digits: stat.digits, value: mean(own), versus: null };
    const benched = stat.bench ? games.filter(g => benches[benchKey(g)] && me(g)) : [];
    if (stat.bench && benched.length >= MIN_COMPARED) {
      const mine = mean(benched.map(g => stat.of(g, me(g)!)!))!;
      const theirs = mean(benched.map(g => stat.bench!(benches[benchKey(g)])))!;
      // The tile shows the user over the same games it compares.
      tile.value = mine;
      tile.versus = { label: benchLabel, value: theirs, delta: mine - theirs, games: benched.length };
      return tile;
    }
    const pairs = games.flatMap(g => {
      const p = me(g), o = opponent(g);
      const a = p && stat.of(g, p), b = o && stat.of(g, o);
      return a == null || b == null ? [] : [[a, b]];
    });
    if (pairs.length >= MIN_COMPARED) {
      const [mine, theirs] = [mean(pairs.map(p => p[0]))!, mean(pairs.map(p => p[1]))!];
      tile.value = mine;
      tile.versus = { label: "lane opp.", value: theirs, delta: mine - theirs, games: pairs.length };
    }
    return tile;
  });
}

export function formatStat(value: number, digits: number, unit = "") {
  return `${value.toLocaleString("en-US", { minimumFractionDigits: digits, maximumFractionDigits: digits })}${unit}`;
}

/** How a value compares with its reference: within 5% reads as even. */
export function tone(value: number, reference: number): "up" | "even" | "down" {
  const diff = value - reference;
  if (Math.abs(diff) <= 0.05 * Math.abs(reference)) return "even";
  return diff > 0 ? "up" : "down";
}

/** The Solo/Duo bracket label for a tier, matching the benchmark brackets. */
export function tierLabel(tier: string | undefined) {
  const t = (tier ?? "").toUpperCase();
  if (["MASTER", "GRANDMASTER", "CHALLENGER"].includes(t)) return "Master+";
  return t && t !== "NONE" ? t.charAt(0) + t.slice(1).toLowerCase() : "all ranks";
}

export type Today = { games: number; wins: number; losses: number; lp: number | null; seconds: number; since: number | null };

const APEX = ["MASTER", "GRANDMASTER", "CHALLENGER"];
const onLadder = (tier: string) => LADDER.includes(tier) || APEX.includes(tier);

/** Today's net LP: ladder score now (the live `rank`) minus the score at the last
 *  record before today's first Solo/Duo game, else the first record of today.
 *  Null without a ranked game today, a current rank or such a baseline: never a partial sum. */
function lpToday(played: Game[], midnight: number, rank: Snapshot["rank"], history: LpPoint[]) {
  const first = Math.min(...played.filter(g => g.queue === "solo").map(g => g.createdAt));
  if (!rank || !onLadder(rank.tier) || !Number.isFinite(first)) return null;
  const points = history.filter(p => onLadder(p.tier)).sort((a, b) => a.at - b.at);
  const base = points.filter(p => p.at < first).pop() ?? points.find(p => p.at >= midnight);
  return base ? ladderScore(rank) - ladderScore(base) : null;
}

/** The session summary: every game since local midnight, any queue. LP covers
 *  Solo/Duo only, taken from the recorded LP history (see `lpToday`). */
export function today(games: Game[], now: number, rank: Snapshot["rank"] = null, history: LpPoint[] = []): Today {
  const midnight = new Date(now).setHours(0, 0, 0, 0);
  const played = games.filter(g => g.createdAt >= midnight);
  return {
    games: played.length, wins: played.filter(g => g.win).length, losses: played.filter(g => !g.win).length,
    lp: lpToday(played, midnight, rank, history),
    seconds: sum(played.map(g => g.duration)),
    since: played.length ? Math.min(...played.map(g => g.createdAt)) : null,
  };
}

/** 1h 52m, or 48m under an hour. */
export function playTime(seconds: number) {
  const minutes = Math.round(seconds / 60);
  return minutes >= 60 ? `${Math.floor(minutes / 60)}h ${String(minutes % 60).padStart(2, "0")}m` : `${minutes}m`;
}

const LADDER = ["IRON", "BRONZE", "SILVER", "GOLD", "PLATINUM", "EMERALD", "DIAMOND"];
const DIVISIONS = ["IV", "III", "II", "I"];
const title = (tier: string) => tier.charAt(0) + tier.slice(1).toLowerCase();

/** Ladder position in LP, mirroring `score` in lp.rs: 100 per division, apex tiers share the top base. */
export function ladderScore(rank: Pick<LpPoint, "tier" | "division" | "lp">) {
  const tier = LADDER.indexOf(rank.tier);
  const division = Math.max(0, DIVISIONS.indexOf(rank.division));
  return (tier < 0 ? LADDER.length * 400 : tier * 400 + division * 100) + rank.lp;
}

export type Progress = { lp: number; next: string | null; toNext: number | null };
/** LP bar to the next division. Apex tiers have no division to climb to.
 *  shortcut: Diamond I reads "Master" with a 100 LP bar, the real cut-off moves daily. */
export function rankProgress(rank: NonNullable<Snapshot["rank"]>): Progress {
  const tier = LADDER.indexOf(rank.tier);
  if (tier < 0) return { lp: rank.lp, next: null, toNext: null };
  const division = DIVISIONS.indexOf(rank.division);
  const next = division < 3 ? `${title(rank.tier)} ${DIVISIONS[division + 1]}`
    : tier < LADDER.length - 1 ? `${title(LADDER[tier + 1])} IV` : "Master";
  return { lp: rank.lp, next, toNext: Math.max(0, 100 - rank.lp) };
}

export type LpTrend = { scores: number[]; games: number };
const TREND_POINTS = 20;
/** The recent LP line; null below two points, where there is nothing to draw. */
export function lpTrend(points: LpPoint[] | undefined): LpTrend | null {
  const recent = (points ?? []).slice(-TREND_POINTS);
  if (recent.length < 2) return null;
  const [first, last] = [recent[0], recent[recent.length - 1]];
  return { scores: recent.map(ladderScore), games: last.wins + last.losses - first.wins - first.losses };
}

/** SVG polyline points for `scores` in a width x height box, with `pad` px inside. */
export function sparkline(scores: number[], width: number, height: number, pad = 3) {
  const [low, high] = [Math.min(...scores), Math.max(...scores)];
  const span = high - low || 1;
  return scores.map((value, i) =>
    `${(pad + i * (width - 2 * pad) / (scores.length - 1)).toFixed(1)},${(height - pad - (value - low) / span * (height - 2 * pad)).toFixed(1)}`).join(" ");
}

/** How a tile compares with its reference: points for percentages, percent otherwise. */
export function deltaText(tile: Tile) {
  const versus = tile.versus;
  if (!versus) return null;
  const sign = versus.delta >= 0 ? "+" : "−";
  const reference = versus.value >= 1000 ? fmtCompact(versus.value) : formatStat(versus.value, tile.digits, tile.unit);
  if (tile.unit === "%") return `${sign}${Math.abs(Math.round(versus.delta))} pts vs ${versus.label} ${reference}`;
  if (!versus.value) return `vs ${versus.label} ${reference}`;
  return `${sign}${Math.abs(Math.round(100 * versus.delta / versus.value))}% vs ${versus.label} ${reference}`;
}
