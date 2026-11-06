import type { MatchupVod, MomentKind } from "./matchupGuide";
import type { ChampionOption } from "../runes/types";

export function vodPlayerName(player?: string): string | null {
  const name = player?.trim();
  return name && !/^\?+$/.test(name) ? name : null;
}

const MOMENT_ART: Record<MomentKind, string | null> = {
  kill: "kills", death: "/vod-icons/dead_red.png", teamfight: "/vod-icons/teamfight.png", trade: "kills",
  dragon: "dragon-100", baron: "baron-100", herald: "herald-100", grubs: "grub",
  tower: "tower-100", roam: "/vod-icons/roam.png", other: null,
};

export function momentArt(kind?: string): string | null {
  return kind && Object.prototype.hasOwnProperty.call(MOMENT_ART, kind) ? MOMENT_ART[kind as MomentKind] : null;
}

/** Full roster names only; longest alternatives consume overlaps before shorter names. */
export function championsInMoment(label: string, champions: readonly ChampionOption[]): ChampionOption[] {
  const normalize = (value: string) => value.replace(/[’‘]/g, "'").toLowerCase();
  const roster = [...champions].filter(champion => champion.name.trim()).sort((a, b) => b.name.length - a.name.length);
  if (!roster.length) return [];
  const escape = (value: string) => value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const names = roster.map(champion => escape(normalize(champion.name))).join("|");
  const matches = normalize(label).matchAll(new RegExp(`(?<![\\p{L}\\p{N}_])(?:${names})(?![\\p{L}\\p{N}_])`, "gu"));
  const found = new Set<number>();
  return [...matches].flatMap(match => {
    const champion = roster.find(candidate => normalize(candidate.name) === match[0])!;
    if (found.has(champion.id)) return [];
    found.add(champion.id);
    return [champion];
  });
}

export function formatVodTime(seconds: number): string {
  const total = Number.isFinite(seconds) ? Math.max(0, Math.floor(seconds)) : 0;
  const minutes = Math.floor(total / 60), remainder = String(total % 60).padStart(2, "0");
  return minutes < 60 ? `${minutes}:${remainder}` : `${Math.floor(minutes / 60)}:${String(minutes % 60).padStart(2, "0")}:${remainder}`;
}

export function vodKey(vod: Pick<MatchupVod, "videoId" | "startSec">): string {
  return `${vod.videoId}@${vod.startSec ?? 0}`;
}

export function segmentOf(vod: Pick<MatchupVod, "startSec" | "endSec" | "durationSec">): { start: number; end: number } {
  return { start: vod.startSec ?? 0, end: vod.endSec ?? vod.durationSec };
}

export function vodUrl(vod: Pick<MatchupVod, "videoId" | "fixture" | "startSec">, atSec?: number): string | null {
  atSec ??= vod.startSec ?? 0;
  if (vod.fixture || vod.videoId.startsWith("fixture-") || !/^[\w-]{11}$/.test(vod.videoId) || !Number.isFinite(atSec) || atSec < 0) return null;
  return `https://www.youtube.com/watch?v=${vod.videoId}&t=${Math.floor(atSec)}`;
}


type Moment = MatchupVod["moments"][number];
// YouTube seeks to the nearest keyframe, which can land just before the requested second.
const SEEK_SLACK_SEC = 1;
// "Previous moment" past this many seconds into a moment restarts it, like a music player.
const RESTART_WINDOW_SEC = 3;

/** The moment playing at `time` (the latest one reached), or -1 before the first. */
export function currentMomentIndex(moments: readonly Moment[], time: number): number {
  let found = -1;
  moments.forEach((moment, i) => {
    if (moment.atSec <= time + SEEK_SLACK_SEC && (found < 0 || moment.atSec >= moments[found].atSec)) found = i;
  });
  return found;
}

export function adjacentMoment(moments: readonly Moment[], time: number, direction: 1 | -1): Moment | undefined {
  const candidates = moments.filter(moment => direction > 0 ? moment.atSec > time + SEEK_SLACK_SEC : moment.atSec < time - RESTART_WINDOW_SEC);
  return candidates.sort((a, b) => direction * (a.atSec - b.atSec))[0];
}

export type MarkerGroup = { atSec: number; indices: number[] };

/** Groups markers that would overlap: each group starts at its earliest moment and takes the moments within `minGapSec` of it. */
export function groupMarkers(moments: readonly Moment[], minGapSec: number): MarkerGroup[] {
  const order = moments.map((_, i) => i).sort((a, b) => moments[a].atSec - moments[b].atSec);
  const groups: MarkerGroup[] = [];
  for (const i of order) {
    const last = groups[groups.length - 1];
    if (last && moments[i].atSec - last.atSec < minGapSec) last.indices.push(i);
    else groups.push({ atSec: moments[i].atSec, indices: [i] });
  }
  return groups;
}

export type PlayerKeyAction = "toggle" | "mute" | "fullscreen" | { seekBy: number } | { moment: 1 | -1 };
type KeyTarget = { tagName?: string; isContentEditable?: boolean } | null;
const SEEK_KEYS = new Map([["j", -10], ["l", 10], ["ArrowLeft", -5], ["ArrowRight", 5]]);

/** YouTube-style shortcuts plus [ and ] for the previous and next key moment. Fields keep their own keys, and Space still presses a focused button. */
export function playerKeyAction(event: { key: string; ctrlKey?: boolean; metaKey?: boolean; altKey?: boolean; target: KeyTarget }): PlayerKeyAction | null {
  const tag = event.target?.tagName?.toUpperCase();
  if (event.ctrlKey || event.metaKey || event.altKey || event.target?.isContentEditable || ["INPUT", "SELECT", "TEXTAREA"].includes(tag ?? "")) return null;
  const key = event.key.length === 1 ? event.key.toLowerCase() : event.key;
  if (key === " " && tag === "BUTTON") return null;
  if (key === " " || key === "k") return "toggle";
  if (key === "m") return "mute";
  if (key === "f") return "fullscreen";
  if (key === "[" || key === "]") return { moment: key === "]" ? 1 : -1 };
  const seekBy = SEEK_KEYS.get(key);
  return seekBy === undefined ? null : { seekBy };
}

export const RANK_TIERS = [["challenger", "Challenger"], ["grandmaster", "Grandmaster"], ["master", "Master"], ["other", "Diamond & below"]] as const;
export type RankTier = typeof RANK_TIERS[number][0];

/** Free-text claims like "KR Grandmaster" or "MASTER"; grandmaster is matched before its substring master. */
export function rankTier(rankClaim: string): RankTier {
  const claim = rankClaim.toLowerCase();
  if (claim.includes("challenger")) return "challenger";
  if (/grand\s*master|\bgm\b/.test(claim)) return "grandmaster";
  return claim.includes("master") ? "master" : "other";
}

// Riot server regions; only a token right before the rank counts, so "vs Master Yi" or a team tag never reads as one.
const REGION_BEFORE_RANK = /\b(EUW|EUNE|KR|NA|BR|LAN|LAS|OCE|TR|RU|JP|VN|TW|SG|PH|TH)\d?\s+(?=(?:challenger|grand\s*master|master|diamond|emerald)\b)/i;

/** Server region as in "KR Grandmaster" or "EUW Challenger Patch 26.19"; explicit region wins, then the rank claim, then the title. */
export function vodRegion(vod: Pick<MatchupVod, "title" | "rankClaim" | "region">): string | null {
  if (vod.region) return vod.region;
  const match = REGION_BEFORE_RANK.exec(vod.rankClaim) ?? REGION_BEFORE_RANK.exec(vod.title);
  return match ? match[1].toUpperCase() : null;
}

export type VodRow = { primary: string; named: boolean; details: string[]; result: "win" | "loss" | null; moments: number | null; rank: string; region: string | null; crest: string | null; patch: string; segment: { start: number; end: number } | null };

/** List row text: the player leads, or rank and region when nobody is named; empty result and moments are left out. */
export function vodRow(vod: MatchupVod): VodRow {
  const name = vodPlayerName(vod.player), region = vodRegion(vod), tier = rankTier(vod.rankClaim);
  const rank = tier === "other" ? vod.rankClaim.replace(REGION_BEFORE_RANK, "").trim() : RANK_TIERS.find(([id]) => id === tier)![1];
  const standing = [region, rank].filter(Boolean).join(" ");
  return {
    rank, region, crest: crestTier(vod.rankClaim), patch: `${vod.patchEstimated ? "~" : ""}${vod.patch}`,
    segment: vod.startSec !== undefined || vod.endSec !== undefined ? segmentOf(vod) : null,
    primary: name ?? (standing || "Ranked game"), named: !!name,
    details: [...(name && standing ? [standing] : []), `Patch ${vod.patchEstimated ? "~" : ""}${vod.patch}`],
    result: vod.result === "W" ? "win" : vod.result === "L" ? "loss" : null,
    moments: vod.moments.length || null,
  };
}

const DAY_MS = 86_400_000;

/** "today", "3 days ago" or "2 weeks ago" for recent uploads, otherwise a short date; null when `uploaded` is not YYYY-MM-DD. */
export function uploadAge(uploaded: string | undefined, now: Date): string | null {
  const at = /^\d{4}-\d{2}-\d{2}$/.test(uploaded ?? "") ? Date.parse(`${uploaded}T00:00:00Z`) : NaN;
  if (!Number.isFinite(at)) return null;
  const days = Math.round((Date.UTC(now.getFullYear(), now.getMonth(), now.getDate()) - at) / DAY_MS);
  const relative = new Intl.RelativeTimeFormat("en", { numeric: "auto" });
  if (days >= 0 && days < 7) return relative.format(-days, "day");
  if (days >= 7 && days < 30) return relative.format(-Math.floor(days / 7), "week");
  return new Date(at).toLocaleDateString("en", { month: "short", day: "numeric", year: "numeric", timeZone: "UTC" });
}

/** Numeric patch order, so 26.10 is newer than 26.9 and 26.21.1 newer than 26.21. */
export function comparePatch(a: string, b: string): number {
  const x = a.split(".").map(Number), y = b.split(".").map(Number);
  for (let i = 0; i < Math.max(x.length, y.length); i++) if ((x[i] || 0) !== (y[i] || 0)) return (x[i] || 0) - (y[i] || 0);
  return 0;
}

/** Opponent and lane restrict the champion library; null means any. */
export type VodFilters = { named: boolean; streamer: boolean; tier: RankTier | null; patch: string | null; enemy: number | null; lane: string | null; region: string | null };
export const ANY_VOD: VodFilters = { named: false, streamer: false, tier: null, patch: null, enemy: null, lane: null, region: null };

export function vodMatches(vod: MatchupVod, f: VodFilters): boolean {
  return (!f.named || !!vodPlayerName(vod.player)) && (!f.streamer || vod.format === "stream") && (!f.tier || rankTier(vod.rankClaim) === f.tier)
    && (!f.region || vodRegion(vod) === f.region) && (!f.patch || vod.patch === f.patch) && (!f.enemy || vod.enemy === f.enemy) && (!f.lane || vod.lane === f.lane);
}

/** Newest patch, then named players, then latest upload; remaining ties keep content order. */
export function filterVods(vods: readonly MatchupVod[], filters: VodFilters): MatchupVod[] {
  return vods.filter(vod => vodMatches(vod, filters)).sort((a, b) => comparePatch(b.patch, a.patch)
    || Number(!!vodPlayerName(b.player)) - Number(!!vodPlayerName(a.player))
    || (b.uploaded ?? "").localeCompare(a.uploaded ?? ""));
}

// Some players get a dark silhouette instead of a 404. Measured on 82 real CDN images:
// headshots have 26% or more bright pixels, silhouettes 8% or fewer.
const PHOTO_MIN_BRIGHT = 0.15;

/** Whether RGBA pixels are a placeholder silhouette (or empty) rather than a real headshot. */
export function isPlaceholderPhoto(rgba: ArrayLike<number>): boolean {
  let opaque = 0, bright = 0;
  for (let i = 0; i + 3 < rgba.length; i += 4) {
    if (rgba[i + 3] <= 128) continue;
    opaque++;
    if ((rgba[i] + rgba[i + 1] + rgba[i + 2]) / 3 > 90) bright++;
  }
  return !opaque || bright / opaque < PHOTO_MIN_BRIGHT;
}

type Clock = { setTimeout: (fn: () => void, ms: number) => unknown; clearTimeout: (id: never) => void };

/** Reports idle after `delayMs` without `activity()`, and active again on the next one. */
export function idleTimer(delayMs: number, onIdle: (idle: boolean) => void, clock: Clock) {
  let timer: unknown, idle = false;
  const activity = () => {
    if (idle) { idle = false; onIdle(false); }
    clock.clearTimeout(timer as never);
    timer = clock.setTimeout(() => { idle = true; onIdle(true); }, delayMs);
  };
  activity();
  return { activity, stop: () => clock.clearTimeout(timer as never) };
}

export function crestTier(claim: string): string | null {
  return /\b(challenger|grandmaster|master|diamond|emerald|platinum|gold|silver|bronze|iron)\b/i.exec(claim)?.[1].toLowerCase() ?? null;
}

/** Each facet ignores its own selection, preserving the pickers' existing counts. */
export function facetCounts(pool: readonly MatchupVod[], filters: VodFilters) {
  const keys = ['named', 'streamer', 'tier', 'patch', 'enemy', 'lane', 'region'] as const;
  const counts = Object.fromEntries(keys.map(key => [key, new Map<unknown, number>()])) as Record<typeof keys[number], Map<unknown, number>>;
  let total = 0;
  for (const vod of pool) {
    const values = { named: !!vodPlayerName(vod.player), streamer: vod.format === 'stream', tier: rankTier(vod.rankClaim), patch: vod.patch, enemy: vod.enemy, lane: vod.lane, region: vodRegion(vod) };
    const passes = keys.map(key => !filters[key] || values[key] === filters[key]);
    const failures = passes.filter(pass => !pass).length;
    if (!failures) total++;
    keys.forEach((key, i) => {
      if (failures - Number(!passes[i])) return;
      const map = counts[key], value = values[key];
      map.set(value, (map.get(value) ?? 0) + 1);
      if (value !== null) map.set(null, (map.get(null) ?? 0) + 1);
    });
  }
  return { ...counts, total };
}

export function playerInitials(name: string): string {
  return name.trim().split(/\s+/).slice(0, 2).map(word => Array.from(word)[0] ?? "").join("").toUpperCase() || "?";
}
export function vodThumbnail(vod: Pick<MatchupVod, "videoId" | "fixture" | "startSec">): string | null {
  return vodUrl(vod) ? `https://i.ytimg.com/vi/${vod.videoId}/mqdefault.jpg` : null;
}
export function vodLabel(vod: MatchupVod, names: { champion: string; enemy: string | null; lane: string | null }, now: Date): string {
  const row = vodRow(vod), seg = segmentOf(vod);
  const tags = [vod.team, vod.format === "stream" ? `streamer${vod.cam ? " with facecam" : ""}` : null].filter(Boolean);
  const facts = [[row.region, row.rank].filter(Boolean).join(" "), row.patch ? `patch ${row.patch}${vod.patchEstimated ? " (estimated)" : ""}` : null, uploadAge(vod.uploaded, now)].filter(Boolean).join(", ");
  return [`Watch ${vodPlayerName(vod.player) ?? "Ranked replay"}${tags.length ? ` (${tags.join(", ")})` : ""}, ${names.champion} vs ${names.enemy ?? "any opponent"}${names.lane ? `, ${names.lane}` : ""}`, facts, row.moments ? `${row.moments} key moment${row.moments === 1 ? "" : "s"}` : null, row.result ? row.result === "win" ? "Win" : "Loss" : null, `Length ${formatVodTime(seg.end - seg.start)}${row.segment ? " (game segment)" : ""}`].filter(Boolean).join(". ") + ".";
}

/** VOD data stores team tags; the logo CDN is keyed by the full team name. Unknown tags pass through. */
const TEAM_NAMES: Record<string, string> = {
  HLE: "Hanwha Life Esports", DK: "Dplus KIA", GEN: "Gen.G Esports", "Gen.G": "Gen.G Esports", BLG: "Bilibili Gaming",
  KT: "KT Rolster", JDG: "JD Gaming", G2: "G2 Esports", KC: "Karmine Corp", NS: "Nongshim RedForce", FNC: "Fnatic",
  AL: "Anyone's Legend", LNG: "LNG Esports", WBG: "Weibo Gaming", IG: "Invictus Gaming", BRO: "OKSavingsBank BRION",
  BFX: "BNK FEARX", EDG: "EDward Gaming", TES: "Top Esports", WE: "Team WE", C9: "Cloud9", FLY: "FlyQuest",
};
export function teamName(tag: string): string {
  return TEAM_NAMES[tag] ?? tag;
}
