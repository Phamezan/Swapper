import type { MatchupVod } from './matchupGuide';

export const YOUTUBE_ORIGIN = 'https://www.youtube-nocookie.com';
export type PlayerInfo = { currentTime?: number; duration?: number; playerState?: number; muted?: boolean; playbackRate?: number; volume?: number; videoLoadedFraction?: number };
const LIMITS: Partial<Record<keyof PlayerInfo, number>> = { volume: 100, videoLoadedFraction: 1 };

export function momentSeek(moment: MatchupVod['moments'][number]) {
  return { event: 'command', func: 'seekTo', args: [moment.atSec, true] };
}

export function playerMessage(event: Pick<MessageEvent, 'source' | 'origin' | 'data'>, source: MessageEventSource | null): PlayerInfo | null {
  if (!source || event.source !== source || ![YOUTUBE_ORIGIN, 'https://www.youtube.com'].includes(event.origin)) return null;
  let message;
  try { message = typeof event.data === 'string' ? JSON.parse(event.data) : event.data; } catch { return null; }
  if (!message || !['infoDelivery', 'initialDelivery'].includes(message.event) || !message.info || typeof message.info !== 'object') return null;
  const info: PlayerInfo = {};
  for (const key of ['currentTime', 'duration', 'playbackRate', 'volume', 'videoLoadedFraction'] as const) {
    const value = message.info[key];
    if (typeof value === 'number' && Number.isFinite(value) && value >= 0 && value <= (LIMITS[key] ?? Infinity) && (key !== 'playbackRate' || value > 0)) info[key] = value;
  }
  if ([-1, 0, 1, 2, 3, 5].includes(message.info.playerState)) info.playerState = message.info.playerState;
  if (typeof message.info.muted === 'boolean') info.muted = message.info.muted;
  return info;
}

/** How the embed is loaded: our controls or YouTube's own (gear menu for quality and captions), resuming at `startSec`. */
export type EmbedMode = { youtubeControls: boolean; startSec: number; autoplay: boolean };

export function embedUrl(videoId: string, mode: EmbedMode, origin: string): string {
  const params = new URLSearchParams({ enablejsapi: '1', controls: mode.youtubeControls ? '1' : '0', fs: '0', rel: '0', playsinline: '1', autoplay: mode.autoplay ? '1' : '0',
    // Undocumented quality hint only: YouTube may ignore it and still picks quality from the player size.
    vq: 'hd1080', origin });
  if (mode.startSec >= 1) params.set('start', String(Math.floor(mode.startSec)));
  return `${YOUTUBE_ORIGIN}/embed/${videoId}?${params}`;
}

/** Switching controls reloads the embed, so it resumes where the video was and keeps playing only if it was (buffering counts as playing). */
export function switchEmbed(youtubeControls: boolean, info: PlayerInfo): EmbedMode {
  return { youtubeControls, startSec: info.currentTime ?? 0, autoplay: info.playerState === 1 || info.playerState === 3 };
}
