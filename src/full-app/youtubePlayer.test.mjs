import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import ts from 'typescript';

const exports = {};
const code = ts.transpileModule(readFileSync(new URL('./youtubePlayer.ts', import.meta.url), 'utf8'), { compilerOptions: { module: ts.ModuleKind.CommonJS } }).outputText;
new Function('exports', code)(exports);
const { momentSeek, playerMessage } = exports;

test('a key moment maps to the exact YouTube seek command', () => {
  assert.deepEqual(momentSeek({ atSec: 184, label: 'Trade' }), { event: 'command', func: 'seekTo', args: [184, true] });
  assert.deepEqual(momentSeek({ atSec: 3723, label: 'Fight' }).args, [3723, true]);
});

test('player messages accept only the iframe window and exact YouTube origins', () => {
  const frame = {};
  const data = JSON.stringify({ event: 'infoDelivery', info: { currentTime: 184, duration: 1746, playerState: 1, muted: true, playbackRate: 1.5 } });
  const message = { source: frame, origin: 'https://www.youtube-nocookie.com', data };
  assert.deepEqual(playerMessage(message, frame), { currentTime: 184, duration: 1746, playerState: 1, muted: true, playbackRate: 1.5 });
  assert.ok(playerMessage({ ...message, origin: 'https://www.youtube.com' }, frame));
  for (const origin of ['https://evil.io', 'https://www.youtube.com.evil.io', 'http://www.youtube.com', 'null']) assert.equal(playerMessage({ ...message, origin }, frame), null);
  assert.equal(playerMessage({ ...message, source: {} }, frame), null);
  for (const value of ['broken', 'null', '{}', '{"event":"infoDelivery","info":null}']) assert.equal(playerMessage({ ...message, data: value }, frame), null);
  assert.deepEqual(playerMessage({ ...message, data: { event: 'infoDelivery', info: { currentTime: -1, duration: Infinity, playerState: '1', playbackRate: 0, muted: 'yes' } } }, frame), {});
});

test('buffered fraction and volume are kept only inside their ranges', () => {
  const frame = {};
  const info = value => playerMessage({ source: frame, origin: 'https://www.youtube-nocookie.com', data: { event: 'infoDelivery', info: value } }, frame);
  assert.deepEqual(info({ volume: 40, videoLoadedFraction: 0.25 }), { volume: 40, videoLoadedFraction: 0.25 });
  assert.deepEqual(info({ volume: 101, videoLoadedFraction: 1.5 }), {});
  assert.deepEqual(info({ volume: -1, videoLoadedFraction: NaN }), {});
});
