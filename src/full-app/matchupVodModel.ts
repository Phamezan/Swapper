import type { MatchupVod, MomentKind } from "./matchupGuide";
import type { ChampionOption } from "../runes/types";

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
