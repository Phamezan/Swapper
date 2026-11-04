export type MiniPlacement = { x: number; y: number; width: number };
export type MiniBounds = { left: number; top: number; right: number; bottom: number };
export const MINI_KEY = 'swapper.vodMiniPlacement';
export const MINI_HEADER = 66;
export function clampMini(value: MiniPlacement | null, bounds: MiniBounds): MiniPlacement {
  const available = Math.max(0, bounds.right - bounds.left);
  const max = Math.max(0, Math.min(available, available * .6, (bounds.bottom - bounds.top - MINI_HEADER) * 16 / 9));
  const width = Math.max(Math.min(280, max), Math.min(value?.width ?? 360, max));
  const height = width * 9 / 16 + MINI_HEADER;
  return { width, x: Math.max(bounds.left, Math.min(value?.x ?? bounds.right - width - 24, bounds.right - width)),
    y: Math.max(bounds.top, Math.min(value?.y ?? bounds.bottom - height - 24, bounds.bottom - height)) };
}
export function parseMini(raw: string | null): MiniPlacement | null {
  try { const value = JSON.parse(raw ?? 'null'); return value && ['x', 'y', 'width'].every(key => typeof value[key] === 'number' && Number.isFinite(value[key])) && value.width > 0 ? value : null; } catch { return null; }
}
export function serializeMini(value: MiniPlacement | null): string { return JSON.stringify(value); }
