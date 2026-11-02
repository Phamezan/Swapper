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

export function vodUrl(vod: Pick<MatchupVod, "videoId" | "fixture">, atSec = 0): string | null {
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

export type PlayerKeyAction = "toggle" | "mute" | "fullscreen" | { seekBy: number };
type KeyTarget = { tagName?: string; isContentEditable?: boolean } | null;
const SEEK_KEYS = new Map([["j", -10], ["l", 10], ["ArrowLeft", -5], ["ArrowRight", 5]]);

/** YouTube-style shortcuts. Inputs (including the volume slider) keep their own keys, and Space still presses a focused button. */
export function playerKeyAction(event: { key: string; ctrlKey?: boolean; metaKey?: boolean; altKey?: boolean; target: KeyTarget }): PlayerKeyAction | null {
  const tag = event.target?.tagName?.toUpperCase();
  if (event.ctrlKey || event.metaKey || event.altKey || event.target?.isContentEditable || ["INPUT", "SELECT", "TEXTAREA"].includes(tag ?? "")) return null;
  const key = event.key.length === 1 ? event.key.toLowerCase() : event.key;
  if (key === " " && tag === "BUTTON") return null;
  if (key === " " || key === "k") return "toggle";
  if (key === "m") return "mute";
  if (key === "f") return "fullscreen";
  const seekBy = SEEK_KEYS.get(key);
  return seekBy === undefined ? null : { seekBy };
}

export const RANK_TIERS = [["challenger", "Challenger"], ["grandmaster", "Grandmaster"], ["master", "Master"], ["other", "Other"]] as const;
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

/** Server region as in "KR Grandmaster" or "EUW Challenger Patch 26.19"; the rank claim wins over the title. */
export function vodRegion(vod: Pick<MatchupVod, "title" | "rankClaim">): string | null {
  const match = REGION_BEFORE_RANK.exec(vod.rankClaim) ?? REGION_BEFORE_RANK.exec(vod.title);
  return match ? match[1].toUpperCase() : null;
}

export type VodRow = { primary: string; named: boolean; details: string[]; result: "win" | "loss" | null; moments: number | null };

/** List row text: the player leads, or rank and region when nobody is named; empty result and moments are left out. */
export function vodRow(vod: MatchupVod): VodRow {
  const name = vodPlayerName(vod.player), region = vodRegion(vod), tier = rankTier(vod.rankClaim);
  const rank = tier === "other" ? vod.rankClaim.replace(REGION_BEFORE_RANK, "").trim() : RANK_TIERS.find(([id]) => id === tier)![1];
  const standing = [region, rank].filter(Boolean).join(" ");
  return {
    primary: name ?? (standing || "Ranked game"), named: !!name,
    details: [...(name && standing ? [standing] : []), `Patch ${vod.patch}`],
    result: vod.result === "W" ? "win" : vod.result === "L" ? "loss" : null,
    moments: vod.moments.length || null,
  };
}

/** Numeric patch order, so 26.10 is newer than 26.9 and 26.21.1 newer than 26.21. */
export function comparePatch(a: string, b: string): number {
  const x = a.split(".").map(Number), y = b.split(".").map(Number);
  for (let i = 0; i < Math.max(x.length, y.length); i++) if ((x[i] || 0) !== (y[i] || 0)) return (x[i] || 0) - (y[i] || 0);
  return 0;
}

/** `null` means any; `enemy` only matches champion-wide VODs, which carry their matchup. */
export type VodFilters = { pros: boolean; tier: RankTier | null; result: "W" | "L" | null; patch: string | null; enemy: number | null };
export const ANY_VOD: VodFilters = { pros: false, tier: null, result: null, patch: null, enemy: null };

export function vodMatches(vod: MatchupVod, f: VodFilters): boolean {
  return (!f.pros || !!vod.team) && (!f.tier || rankTier(vod.rankClaim) === f.tier) && (!f.result || vod.result === f.result)
    && (!f.patch || vod.patch === f.patch) && (!f.enemy || vod.enemy === f.enemy);
}

/** Matching VODs, newest patch first; ties keep the content order (the sort is stable). */
export function filterVods(vods: readonly MatchupVod[], filters: VodFilters): MatchupVod[] {
  return vods.filter(vod => vodMatches(vod, filters)).sort((a, b) => comparePatch(b.patch, a.patch));
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
