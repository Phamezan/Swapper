import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import ts from 'typescript';

const exports = {};
const code = ts.transpileModule(readFileSync(new URL('./youtubePlayer.ts', import.meta.url), 'utf8'), { compilerOptions: { module: ts.ModuleKind.CommonJS } }).outputText;
new Function('exports', code)(exports);
const { embedUrl, playerMessage } = exports;

test('player messages accept only the iframe window and exact YouTube origins', () => {
  const frame = {};
  const data = JSON.stringify({ event: 'infoDelivery', info: { currentTime: 184, duration: 1746, playerState: 1, muted: true, playbackRate: 1.5, volume: 40 } });
  const message = { source: frame, origin: 'https://www.youtube-nocookie.com', data };
  assert.deepEqual(playerMessage(message, frame), { currentTime: 184, duration: 1746, playerState: 1, muted: true }, 'only what the rail and shortcuts use');
  assert.ok(playerMessage({ ...message, origin: 'https://www.youtube.com' }, frame));
  for (const origin of ['https://evil.io', 'https://www.youtube.com.evil.io', 'http://www.youtube.com', 'null']) assert.equal(playerMessage({ ...message, origin }, frame), null);
  assert.equal(playerMessage({ ...message, source: {} }, frame), null);
  for (const value of ['broken', 'null', '{}', '{"event":"infoDelivery","info":null}']) assert.equal(playerMessage({ ...message, data: value }, frame), null);
  assert.deepEqual(playerMessage({ ...message, data: { event: 'infoDelivery', info: { currentTime: -1, duration: Infinity, playerState: '1', muted: 'yes' } } }, frame), {});
});

test("embed URL always uses YouTube's own controls, keeps fullscreen ours and sends no quality hint", () => {
  const url = new URL(embedUrl('A_b-cD12345', 'http://tauri.localhost'));
  assert.equal(url.origin + url.pathname, 'https://www.youtube-nocookie.com/embed/A_b-cD12345');
  assert.deepEqual(Object.fromEntries(url.searchParams), { enablejsapi: '1', controls: '1', fs: '0', rel: '0', playsinline: '1', autoplay: '1', origin: 'http://tauri.localhost' });
});
