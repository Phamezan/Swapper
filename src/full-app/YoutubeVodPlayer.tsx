import { useEffect, useRef, useState } from 'react';
import type { MatchupVod } from './matchupGuide';
import { formatVodTime, vodUrl } from './matchupVodModel';
import { momentSeek, playerMessage, YOUTUBE_ORIGIN, type PlayerInfo } from './youtubePlayer';

export type VodSeek = { moment: MatchupVod['moments'][number]; serial: number };

export function YoutubeVodPlayer({ vod, seek }: { vod: MatchupVod; seek: VodSeek | null }) {
  const frame = useRef<HTMLIFrameElement>(null);
  const [info, setInfo] = useState<PlayerInfo>({});
  const [ready, setReady] = useState(false);
  const [failed, setFailed] = useState(false);
  const send = (message: object) => frame.current?.contentWindow?.postMessage(JSON.stringify(message), YOUTUBE_ORIGIN);
  const command = (func: string, args: unknown[] = []) => send({ event: 'command', func, args });
  useEffect(() => {
    if (!vodUrl(vod)) return;
    let connected = false;
    const receive = (event: MessageEvent) => {
      const next = playerMessage(event, frame.current?.contentWindow ?? null);
      if (next) { connected = true; setReady(true); setFailed(false); setInfo(previous => ({ ...previous, ...next })); }
      if (event.source === frame.current?.contentWindow && [YOUTUBE_ORIGIN, 'https://www.youtube.com'].includes(event.origin)) {
        try {
          const data = typeof event.data === 'string' ? JSON.parse(event.data) : event.data;
          if (data?.event === 'onError') setFailed(true);
        } catch { /* Ignore unrelated messages. */ }
      }
    };
    window.addEventListener('message', receive);
    const timer = window.setInterval(() => { if (!connected) send({ event: 'listening', id: 1 }); }, 500);
    const timeout = window.setTimeout(() => { if (!connected) setFailed(true); }, 20000);
    return () => { window.removeEventListener('message', receive); window.clearInterval(timer); window.clearTimeout(timeout); };
  }, [vod.videoId, vod.fixture]);
  useEffect(() => {
    if (!ready || !seek) return;
    send(momentSeek(seek.moment)); command('playVideo');
  }, [ready, seek]);
  if (!vodUrl(vod)) return <div className="mv-frame mv-preview-frame"><span className="mv-preview">Preview, no video</span><p>Preview only · no video linked</p></div>;
  const duration = info.duration || vod.durationSec;
  const current = Math.min(info.currentTime ?? 0, duration);
  return <div className="mv-video-player">
    <div className="mv-frame mv-embed"><iframe ref={frame} title={vod.title}
      src={`${YOUTUBE_ORIGIN}/embed/${vod.videoId}?enablejsapi=1&controls=0&rel=0&playsinline=1&autoplay=1&origin=${encodeURIComponent(location.origin)}`}
      allow="autoplay; encrypted-media; picture-in-picture; fullscreen" allowFullScreen
      onLoad={() => { setFailed(false); send({ event: 'listening', id: 1 }); }} /></div>
    <div className="mv-controls" aria-label="Video controls">
      <div className="mv-scrubber"><input type="range" min="0" max={duration} step="1" value={current} disabled={!ready}
        aria-label="Video position" aria-valuetext={`${formatVodTime(current)} of ${formatVodTime(duration)}`}
        onChange={event => command('seekTo', [Number(event.target.value), true])} />
        <div className="mv-markers">{vod.moments.map((moment, i) => <button key={i} type="button" disabled={!ready}
          style={{ left: `${moment.atSec / duration * 100}%` }} title={`${formatVodTime(moment.atSec)} · ${moment.label}`}
          aria-label={`Jump to ${moment.label} at ${formatVodTime(moment.atSec)}`} onClick={() => { send(momentSeek(moment)); command('playVideo'); }} />)}</div>
      </div>
      <div className="mv-control-row"><button type="button" disabled={!ready} onClick={() => command(info.playerState === 1 ? 'pauseVideo' : 'playVideo')}>{info.playerState === 1 ? 'Pause' : 'Play'}</button>
        <span className="mv-time">{formatVodTime(current)} / {formatVodTime(duration)}</span>
        <label>Speed <select aria-label="Playback speed" value={info.playbackRate ?? 1} disabled={!ready} onChange={event => command('setPlaybackRate', [Number(event.target.value)])}>{[0.25, 0.5, 0.75, 1, 1.25, 1.5, 1.75, 2].map(rate => <option key={rate} value={rate}>{rate}×</option>)}</select></label>
        <button type="button" disabled={!ready} onClick={() => command(info.muted ? 'unMute' : 'mute')}>{info.muted ? 'Unmute' : 'Mute'}</button>
      </div>
      {failed && <p role="status">Video unavailable or slow to load. Try opening it on YouTube.</p>}
    </div>
  </div>;
}
