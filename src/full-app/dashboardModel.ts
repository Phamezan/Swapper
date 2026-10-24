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

const DAY = 864e5;
const GRAPH_DAYS = 30;
const APEX_SCORE = LADDER.length * 400;

/** Tier name for a ladder score; every apex score is Master and up. */
const tierAt = (score: number) => score >= APEX_SCORE ? "MASTER" : LADDER[Math.floor(score / 400)];

export type GraphPoint = { x: number; y: number; point: LpPoint; /** Games since the previous point. */ games: number };
export type LpGraph = {
  points: GraphPoint[];
  /** Smooth SVG path through the points; monotone, so it never bulges past a recorded LP. */
  path: string;
  /** The tier each height range belongs to, as fractions from the top, for the line's color stops. */
  tiers: { from: number; to: number; tier: string }[];
  /** LP gained (negative when lost) over the last 30 and 7 days; null without a point to compare with. */
  month: number | null;
  week: number | null;
  peak: LpPoint;
  /** Division lines; `tier` is set where a tier starts, which shows its crest. */
  guides: { y: number; label: string; tier: string | null }[];
};

/** Monotone cubic (Fritsch-Carlson) through points sorted by x, as an SVG path. */
export function smoothPath(points: { x: number; y: number }[]) {
  const n = points.length;
  if (!n) return "";
  const slope = (i: number) => {
    const dx = points[i + 1].x - points[i].x;
    return dx ? (points[i + 1].y - points[i].y) / dx : 0;
  };
  const m = points.slice(0, -1).map((_, i) => slope(i));
  const t = points.map((_, i) => i === 0 ? m[0] : i === n - 1 ? m[n - 2] : m[i - 1] * m[i] <= 0 ? 0 : (m[i - 1] + m[i]) / 2);
  for (let i = 0; i < n - 1; i++) {
    if (m[i] === 0) { t[i] = 0; t[i + 1] = 0; continue; }
    const [a, b] = [t[i] / m[i], t[i + 1] / m[i]];
    const s = a * a + b * b;
    if (s > 9) { const k = 3 / Math.sqrt(s); t[i] = k * a * m[i]; t[i + 1] = k * b * m[i]; }
  }
  const f = (v: number) => v.toFixed(1);
  let d = `M${f(points[0].x)},${f(points[0].y)}`;
  for (let i = 0; i < n - 1; i++) {
    const [p, q] = [points[i], points[i + 1]];
    const h = (q.x - p.x) / 3;
    d += ` C${f(p.x + h)},${f(p.y + h * t[i])} ${f(q.x - h)},${f(q.y - h * t[i + 1])} ${f(q.x)},${f(q.y)}`;
  }
  return d;
}

/** The Solo/Duo LP line over the last 30 days; null below two points, where there is nothing to draw. */
export function lpGraph(history: LpPoint[] | undefined, now: number, width: number, height: number): LpGraph | null {
  const sorted = (history ?? []).filter(p => onLadder(p.tier) && Number.isFinite(p.at) && Number.isFinite(p.lp) && p.lp >= 0 && p.at <= now)
    .sort((a, b) => a.at - b.at);
  const all = sorted.filter((p, i) => p.at !== sorted[i + 1]?.at);
  const recent = all.filter(p => p.at >= now - GRAPH_DAYS * DAY);
  if (recent.length < 2) return null;
  const scores = recent.map(ladderScore);
  // Frame the recorded range with division lines and room around promotions.
  const padding = Math.max(24, (Math.max(...scores) - Math.min(...scores)) * .1);
  const low = Math.max(0, Math.floor((Math.min(...scores) - padding) / 100) * 100);
  const high = Math.ceil((Math.max(...scores) + padding) / 100) * 100;
  const [start, end] = [recent[0].at, recent[recent.length - 1].at];
  const y = (score: number) => height - (score - low) / (high - low) * height;
  const points = recent.map((p, i) => ({
    x: (p.at - start) / (end - start || 1) * width, y: y(scores[i]), point: p,
    // Seeded points have no game counts, so no count spans them.
    games: i && !p.source && !recent[i - 1].source ? Math.max(0, p.wins + p.losses - recent[i - 1].wins - recent[i - 1].losses) : 0,
  }));
  const latest = scores[scores.length - 1];
  // The last point before the window is the fairest baseline; else the window's first point.
  const gain = (days: number) => {
    const cutoff = now - days * DAY;
    const base = [...all].reverse().find(p => p.at <= cutoff) ?? recent.find(p => p.at > cutoff);
    return base && base !== recent[recent.length - 1] ? latest - ladderScore(base) : null;
  };
  const tiers = [];
  // Apex tiers have no 400 LP floors, so Master and up is one band.
  for (let floor = low < APEX_SCORE ? low - low % 400 : low; floor < high;) {
    const top = floor >= APEX_SCORE ? high : Math.min(high, floor + 400);
    tiers.push({ from: y(top) / height, to: y(Math.max(low, floor)) / height, tier: tierAt(floor) });
    floor = top;
  }
  const step = high - low > 600 ? 400 : 100;
  // Lines (and crests) only inside divisions the player reached: the padding stays empty, so Emerald I
  // never shows a Diamond crest or a "Diamond III starts here" line above it.
  const reachedLow = Math.floor(Math.min(...scores) / 100) * 100;
  const reachedHigh = Math.floor(Math.max(...scores) / 100) * 100;
  const guides = [];
  for (let score = Math.ceil(Math.max(low, reachedLow) / step) * step; score <= Math.min(high, reachedHigh); score += step) {
    const apex = score >= APEX_SCORE;
    guides.push({
      y: y(score),
      label: apex ? `${score - APEX_SCORE} LP` : DIVISIONS[Math.floor(score % 400 / 100)],
      tier: score === APEX_SCORE || (!apex && score % 400 === 0) ? tierAt(score) : null,
    });
  }
  const peak = recent.reduce((best, p) => ladderScore(p) > ladderScore(best) ? p : best);
  return { points, path: smoothPath(points), tiers: tiers.reverse(), month: gain(GRAPH_DAYS), week: gain(7), peak, guides };
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
