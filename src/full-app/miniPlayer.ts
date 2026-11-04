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

export type MiniCorner = 'nw' | 'ne' | 'sw' | 'se';
export function resizeMini(rect: MiniPlacement, corner: MiniCorner, dx: number, dy: number, bounds: MiniBounds): MiniPlacement {
  const west = corner.includes('w'), north = corner.includes('n'), ratio = 9 / 16;
  const anchorX = west ? rect.x + rect.width : rect.x;
  const anchorY = north ? rect.y + rect.width * ratio + MINI_HEADER : rect.y;
  // Project the pointer onto the aspect-ratio diagonal so either axis can resize.
  const delta = ((west ? -dx : dx) + (north ? -dy : dy) * ratio) / (1 + ratio * ratio);
  const availableWidth = west ? anchorX - bounds.left : bounds.right - anchorX;
  const availableHeight = north ? anchorY - bounds.top : bounds.bottom - anchorY;
  const max = Math.max(0, Math.min(clampMini({ ...rect, width: Infinity }, bounds).width, availableWidth, (availableHeight - MINI_HEADER) / ratio));
  const width = Math.max(Math.min(280, max), Math.min(rect.width + delta, max));
  return { width, x: west ? anchorX - width : anchorX, y: north ? anchorY - width * ratio - MINI_HEADER : anchorY };
}
