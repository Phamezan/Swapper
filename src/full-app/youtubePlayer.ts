export const YOUTUBE_ORIGIN = 'https://www.youtube-nocookie.com';
export type PlayerInfo = { currentTime?: number; duration?: number; playerState?: number; muted?: boolean };

export function playerMessage(event: Pick<MessageEvent, 'source' | 'origin' | 'data'>, source: MessageEventSource | null): PlayerInfo | null {
  if (!source || event.source !== source || ![YOUTUBE_ORIGIN, 'https://www.youtube.com'].includes(event.origin)) return null;
  let message;
  try { message = typeof event.data === 'string' ? JSON.parse(event.data) : event.data; } catch { return null; }
  if (!message || !['infoDelivery', 'initialDelivery'].includes(message.event) || !message.info || typeof message.info !== 'object') return null;
  const info: PlayerInfo = {};
  for (const key of ['currentTime', 'duration'] as const) {
    const value = message.info[key];
    if (typeof value === 'number' && Number.isFinite(value) && value >= 0) info[key] = value;
  }
  if ([-1, 0, 1, 2, 3, 5].includes(message.info.playerState)) info.playerState = message.info.playerState;
  if (typeof message.info.muted === 'boolean') info.muted = message.info.muted;
  return info;
}

/** YouTube's own controls (quality, captions, chapters); `fs=0` because fullscreen is ours, with the moment rail and sidebar. */
export function embedUrl(videoId: string, origin: string): string {
  const params = new URLSearchParams({ enablejsapi: '1', controls: '1', fs: '0', rel: '0', playsinline: '1', autoplay: '1', origin });
  return `${YOUTUBE_ORIGIN}/embed/${videoId}?${params}`;
}
