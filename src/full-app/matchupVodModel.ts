import type { MatchupVod } from "./matchupGuide";

export function formatVodTime(seconds: number): string {
  const total = Number.isFinite(seconds) ? Math.max(0, Math.floor(seconds)) : 0;
  const minutes = Math.floor(total / 60), remainder = String(total % 60).padStart(2, "0");
  return minutes < 60 ? `${minutes}:${remainder}` : `${Math.floor(minutes / 60)}:${String(minutes % 60).padStart(2, "0")}:${remainder}`;
}

export function vodUrl(vod: Pick<MatchupVod, "videoId" | "fixture">, atSec = 0): string | null {
  if (vod.fixture || vod.videoId.startsWith("fixture-") || !/^[\w-]{11}$/.test(vod.videoId) || !Number.isFinite(atSec) || atSec < 0) return null;
  return `https://www.youtube.com/watch?v=${vod.videoId}&t=${Math.floor(atSec)}`;
}

