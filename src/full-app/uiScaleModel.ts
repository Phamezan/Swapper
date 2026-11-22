// The full app is laid out for a 1920x1080 window; bigger windows zoom it up to match.
const BASE_WIDTH = 1920;
const BASE_HEIGHT = 1080;
const MAX_SCALE = 1.5;

/** Zoom for a window of this logical (CSS px at zoom 1) size: never below 1, at most 1.5. */
export function uiScale(width: number, height: number): number {
  const fit = Math.min(width / BASE_WIDTH, height / BASE_HEIGHT);
  if (!Number.isFinite(fit)) return 1;
  return Math.round(Math.min(MAX_SCALE, Math.max(1, fit)) * 100) / 100;
}
