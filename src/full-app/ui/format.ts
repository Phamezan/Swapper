const counts = new Intl.NumberFormat("en-US");

/** Counts with thousands separators: 183,275. Fixed to en-US so the app reads the same on every system locale. */
export function fmtCount(value: number) {
  return counts.format(Math.round(value));
}

/** Large stats (damage, gold) as 22.4k / 1.2M with one decimal; below 1,000 as a plain count. */
export function fmtCompact(value: number) {
  const size = Math.abs(value);
  if (size < 1000) return fmtCount(value);
  // Round first so 999,960 becomes 1.0M rather than 1000.0k.
  const thousands = Math.round(value / 100) / 10;
  if (Math.abs(thousands) < 1000) return `${thousands.toFixed(1)}k`;
  return `${(Math.round(value / 1e5) / 10).toFixed(1)}M`;
}
