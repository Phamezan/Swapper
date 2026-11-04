import { test } from 'node:test';
import assert from 'node:assert/strict';
import { clampMini, resizeMini, parseMini, serializeMini, MINI_HEADER } from './miniPlayer.ts';
const bounds = { left: 180, top: 40, right: 1280, bottom: 820 };
test('mini placement clamps position and size within app bounds', () => {
  const rect = clampMini({ x: -900, y: 9000, width: 9000 }, bounds);
  assert.equal(rect.width, 660); assert.equal(rect.x, 180);
  assert.equal(rect.y + rect.width * 9 / 16 + MINI_HEADER, bounds.bottom);
  assert.equal(clampMini({ x: 500, y: 100, width: 1 }, bounds).width, 280);
  const small = { left: 0, top: 0, right: 240, bottom: 180 };
  const tiny = clampMini(rect, small);
  assert.ok(tiny.x >= 0 && tiny.y >= 0 && tiny.x + tiny.width <= 240 && tiny.y + tiny.width * 9 / 16 + MINI_HEADER <= 180);
});
test('mini placement persists, rejects invalid storage and resets to bottom right', () => {
  const moved = { x: 200, y: 100, width: 300 };
  assert.deepEqual(parseMini(serializeMini(moved)), moved);
  for (const raw of [null, 'bad', '{}', '{"x":0,"y":0,"width":-1}', '{"x":"0","y":0,"width":300}']) assert.equal(parseMini(raw), null);
  assert.equal(parseMini(serializeMini(null)), null);
  const reset = clampMini(parseMini(serializeMini(null)), bounds);
  assert.equal(reset.width, 360); assert.equal(reset.x + reset.width, bounds.right - 24);
  assert.equal(reset.y + reset.width * 9 / 16 + MINI_HEADER, bounds.bottom - 24);
});

for (const corner of ['nw', 'ne', 'sw', 'se']) {
  test(`mini resize from ${corner} preserves opposite corner and clamps size`, () => {
    const rect = { x: 480, y: 280, width: 360 };
    const anchor = value => [corner.includes('w') ? value.x + value.width : value.x,
      corner.includes('n') ? value.y + value.width * 9 / 16 + MINI_HEADER : value.y];
    for (const amount of [60, -60, 10000, -10000]) {
      const resized = resizeMini(rect, corner, (corner.includes('w') ? -1 : 1) * amount, (corner.includes('n') ? -1 : 1) * amount * 9 / 16, bounds);
      assert.deepEqual(anchor(resized), anchor(rect));
      assert.ok(resized.width >= 280 && resized.width <= 660);
      assert.ok(resized.x >= bounds.left && resized.y >= bounds.top);
      assert.ok(resized.x + resized.width <= bounds.right && resized.y + resized.width * 9 / 16 + MINI_HEADER <= bounds.bottom);
      if (amount === 60) assert.equal(resized.width, 420);
      if (amount === -60) assert.equal(resized.width, 300);
    }
  });
}
