import { useEffect, useRef, useState, type ButtonHTMLAttributes, type CSSProperties, type KeyboardEvent, type PointerEvent } from 'react';
import { Maximize, Minimize, Pause, Play, RotateCcw, RotateCw, SkipBack, SkipForward, Volume1, Volume2, VolumeX } from 'lucide-react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import type { MatchupVod } from './matchupGuide';
import { adjacentMoment, currentMomentIndex, formatVodTime, groupMarkers, playerKeyAction, vodUrl } from './matchupVodModel';
import { MomentIcon } from './MomentIcon';
import { playerMessage, YOUTUBE_ORIGIN, type PlayerInfo } from './youtubePlayer';

type Moment = MatchupVod['moments'][number];
export type VodSeek = { moment: Moment; serial: number };

const RATES = [0.5, 0.75, 1, 1.25, 1.5, 2];
const PLAYING = 1;
// Markers closer than this many pixels merge into one group so their icons never overlap.
const MARKER_SPACING_PX = 26;
// After the browser leaves element fullscreen on Esc, the same key must not also close the VOD.
const ESC_GRACE_MS = 500;

function IconButton({ label, shortcut, children, ...props }: { label: string; shortcut?: string } & ButtonHTMLAttributes<HTMLButtonElement>) {
  return <button type="button" className="mv-icon-button" aria-label={label} aria-keyshortcuts={shortcut} title={shortcut ? `${label} (${shortcut})` : label} {...props}>{children}</button>;
}

function Timeline({ vod, current, duration, buffered, disabled, onSeek, onJump }: {
  vod: MatchupVod; current: number; duration: number; buffered: number; disabled: boolean;
  onSeek: (sec: number, final: boolean) => void; onJump: (sec: number) => void;
}) {
  const lane = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(480);
  const [hover, setHover] = useState<number | null>(null);
  const [dragging, setDragging] = useState(false);
  useEffect(() => {
    const element = lane.current;
    if (!element || typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(([entry]) => setWidth(entry.contentRect.width || 480));
    observer.observe(element);
    return () => observer.disconnect();
  }, []);
  const at = (event: PointerEvent<HTMLDivElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    return Math.max(0, Math.min(1, (event.clientX - rect.left) / rect.width)) * duration;
  };
  const percent = (sec: number) => `${Math.max(0, Math.min(100, sec / duration * 100))}%`;
  return <div className="mv-timeline">
    <div className={`mv-track${dragging ? ' is-dragging' : ''}`} role="slider" tabIndex={disabled ? -1 : 0} aria-disabled={disabled}
      aria-label="Video position" aria-valuemin={0} aria-valuemax={Math.round(duration)} aria-valuenow={Math.round(current)}
      aria-valuetext={`${formatVodTime(current)} of ${formatVodTime(duration)}`}
      onPointerDown={event => {
        if (disabled || event.button !== 0) return;
        event.currentTarget.setPointerCapture(event.pointerId); setDragging(true); onSeek(at(event), false);
      }}
      onPointerMove={event => { const sec = at(event); setHover(sec); if (dragging) onSeek(sec, false); }}
      onPointerUp={event => { if (dragging) { setDragging(false); onSeek(at(event), true); } }}
      onPointerCancel={() => setDragging(false)} onPointerLeave={() => setHover(null)}>
      <span className="mv-rail">
        <span className="mv-rail-buffered" style={{ width: `${buffered * 100}%` }} />
        {hover !== null && <span className="mv-rail-hover" style={{ width: percent(hover) }} />}
        <span className="mv-rail-played" style={{ width: percent(current) }} />
      </span>
      <span className="mv-thumb" style={{ left: percent(current) }} />
      {hover !== null && <span className="mv-hover-time" style={{ left: `clamp(18px, ${percent(hover)}, calc(100% - 18px))` }}>{formatVodTime(hover)}</span>}
    </div>
    <div className="mv-marker-lane" ref={lane}>{groupMarkers(vod.moments, duration * MARKER_SPACING_PX / width).map(group => {
      const members = group.indices.map(i => vod.moments[i]);
      const edge = group.atSec / duration < .25 ? ' is-start' : group.atSec / duration > .75 ? ' is-end' : '';
      return <button key={group.indices[0]} type="button" disabled={disabled} data-kind={members[0].kind ?? 'none'}
        className={`mv-marker${group.atSec <= current ? ' is-past' : ''}${edge}`} style={{ left: percent(group.atSec) }}
        aria-label={`Jump to ${members.map(moment => `${moment.label} at ${formatVodTime(moment.atSec)}`).join('; ')}`} onClick={() => onJump(group.atSec)}>
        <MomentIcon kind={members[0].kind} size={12} />
        {members.length > 1 && <span className="mv-marker-count">{members.length}</span>}
        <span className="mv-tip" aria-hidden>{members.map((moment, i) => <span key={i}>
          <MomentIcon kind={moment.kind} size={12} /><b>{formatVodTime(moment.atSec)}</b><span>{moment.label}</span>
        </span>)}</span>
      </button>;
    })}</div>
  </div>;
}

export function YoutubeVodPlayer({ vod, seek, mini = false, onMomentChange }: {
  vod: MatchupVod; seek: VodSeek | null; mini?: boolean; onMomentChange?: (index: number) => void;
}) {
  const frame = useRef<HTMLIFrameElement>(null);
  const root = useRef<HTMLDivElement>(null);
  const fullscreenNow = useRef(false);
  const fullscreenExitedAt = useRef(-Infinity);
  const [info, setInfo] = useState<PlayerInfo>({});
  const [ready, setReady] = useState(false);
  const [failed, setFailed] = useState(false);
  const [fullscreen, setFullscreen] = useState(false);
  const [speedOpen, setSpeedOpen] = useState(false);
  fullscreenNow.current = fullscreen;
  const duration = info.duration || vod.durationSec;
  const current = Math.min(info.currentTime ?? 0, duration);
  const momentIndex = currentMomentIndex(vod.moments, current);
  const send = (message: object) => frame.current?.contentWindow?.postMessage(JSON.stringify(message), YOUTUBE_ORIGIN);
  const command = (func: string, args: unknown[] = []) => send({ event: 'command', func, args });
  // Seek optimistically: YouTube reports progress a few times a second, so the UI would lag behind the click.
  const seekTo = (sec: number, final = true) => {
    const to = Math.max(0, Math.min(sec, duration));
    command('seekTo', [to, final]);
    setInfo(previous => ({ ...previous, currentTime: to }));
  };
  const jump = (sec: number) => { seekTo(sec); command('playVideo'); };
  const togglePlay = () => command(info.playerState === PLAYING ? 'pauseVideo' : 'playVideo');
  const toggleMute = () => { command(info.muted ? 'unMute' : 'mute'); setInfo(previous => ({ ...previous, muted: !previous.muted })); };
  // The element API puts the player in the top layer of the WebView; wry ignores it, so the Tauri window goes fullscreen too.
  // Element first: it needs the click's user activation, which must not wait behind the window IPC.
  const changeFullscreen = async (on: boolean) => {
    setFullscreen(on);
    try {
      if (on && !document.fullscreenElement) await root.current?.requestFullscreen();
      if (!on && document.fullscreenElement) await document.exitFullscreen();
    } catch { /* Refused element fullscreen: the .is-fullscreen fixed layout still covers the window. */ }
    try { await getCurrentWindow().setFullscreen(on); } catch { /* Outside Tauri the element fullscreen alone fills the page. */ }
  };
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
    jump(seek.moment.atSec);
  }, [ready, seek]);
  useEffect(() => { onMomentChange?.(momentIndex); }, [momentIndex, onMomentChange]);
  useEffect(() => {
    if (!fullscreen) return;
    const sync = () => {
      if (document.fullscreenElement) return;
      fullscreenExitedAt.current = performance.now();
      void changeFullscreen(false);
    };
    document.addEventListener('fullscreenchange', sync);
    return () => document.removeEventListener('fullscreenchange', sync);
  }, [fullscreen]);
  useEffect(() => { if (mini && fullscreen) void changeFullscreen(false); }, [mini, fullscreen]);
  useEffect(() => () => { if (fullscreenNow.current) void getCurrentWindow().setFullscreen(false).catch(() => {}); }, []);
  if (!vodUrl(vod)) return <div className="mv-frame mv-preview-frame"><span className="mv-preview">Preview, no video</span><p>Preview only · no video linked</p></div>;
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === 'Escape' && (speedOpen || fullscreen || performance.now() - fullscreenExitedAt.current < ESC_GRACE_MS)) {
      event.preventDefault(); event.stopPropagation();
      if (speedOpen) setSpeedOpen(false); else if (fullscreen) void changeFullscreen(false);
      return;
    }
    const action = ready && playerKeyAction({ key: event.key, ctrlKey: event.ctrlKey, metaKey: event.metaKey, altKey: event.altKey, target: event.target as HTMLElement });
    if (!action || (action === 'fullscreen' && mini)) return;
    event.preventDefault();
    if (action === 'toggle') togglePlay();
    else if (action === 'mute') toggleMute();
    else if (action === 'fullscreen') void changeFullscreen(!fullscreen);
    else seekTo(current + action.seekBy);
  };
  const playing = info.playerState === PLAYING;
  const volume = info.muted ? 0 : info.volume ?? 100;
  const VolumeIcon = volume === 0 ? VolumeX : volume < 50 ? Volume1 : Volume2;
  const previous = adjacentMoment(vod.moments, current, -1), next = adjacentMoment(vod.moments, current, 1);
  const moment = vod.moments[momentIndex];
  const rate = info.playbackRate ?? 1;
  return <div ref={root} tabIndex={-1} className={`mv-video-player${fullscreen ? ' is-fullscreen' : ''}`} onKeyDown={onKeyDown}>
    <div className="mv-frame mv-embed"><iframe ref={frame} title={vod.title}
      src={`${YOUTUBE_ORIGIN}/embed/${vod.videoId}?enablejsapi=1&controls=0&fs=0&rel=0&playsinline=1&autoplay=1&origin=${encodeURIComponent(location.origin)}`}
      allow="autoplay; encrypted-media; picture-in-picture"
      onLoad={() => { setFailed(false); send({ event: 'listening', id: 1 }); }} /></div>
    <div className="mv-controls" aria-label="Video controls">
      <Timeline vod={vod} current={current} duration={duration} buffered={info.videoLoadedFraction ?? 0} disabled={!ready} onSeek={seekTo} onJump={jump} />
      <div className="mv-control-row">
        <IconButton label="Previous key moment" className="mv-icon-button mv-hide-narrow" disabled={!ready || !previous} onClick={() => previous && jump(previous.atSec)}><SkipBack size={17} /></IconButton>
        <IconButton label="Back 10 seconds" shortcut="j" className="mv-icon-button mv-hide-mini" disabled={!ready} onClick={() => seekTo(current - 10)}><RotateCcw size={17} /></IconButton>
        <IconButton label={playing ? 'Pause' : 'Play'} shortcut="k" className="mv-icon-button mv-play" disabled={!ready} onClick={togglePlay}>{playing ? <Pause size={18} fill="currentColor" /> : <Play size={18} fill="currentColor" />}</IconButton>
        <IconButton label="Forward 10 seconds" shortcut="l" className="mv-icon-button mv-hide-mini" disabled={!ready} onClick={() => seekTo(current + 10)}><RotateCw size={17} /></IconButton>
        <IconButton label="Next key moment" className="mv-icon-button mv-hide-narrow" disabled={!ready || !next} onClick={() => next && jump(next.atSec)}><SkipForward size={17} /></IconButton>
        <span className="mv-volume">
          <IconButton label={info.muted ? 'Unmute' : 'Mute'} shortcut="m" disabled={!ready} onClick={toggleMute}><VolumeIcon size={17} /></IconButton>
          <input type="range" min="0" max="100" step="1" value={volume} disabled={!ready} aria-label="Volume" style={{ '--mv-fill': `${volume}%` } as CSSProperties}
            onChange={event => {
              const value = Number(event.target.value);
              command('setVolume', [value]);
              if (info.muted && value > 0) command('unMute');
              setInfo(previous => ({ ...previous, volume: value, muted: value === 0 ? previous.muted : false }));
            }} />
        </span>
        <span className="mv-time">{formatVodTime(current)} / {formatVodTime(duration)}</span>
        <span className="mv-now" title={moment?.label}>{moment && <><MomentIcon kind={moment.kind} /><span>{moment.label}</span></>}</span>
        <span className="mv-speed mv-hide-mini" onBlur={event => { if (!event.currentTarget.contains(event.relatedTarget)) setSpeedOpen(false); }}>
          <button type="button" className="mv-speed-button" aria-label={`Playback speed ${rate}×`} title="Playback speed" aria-haspopup="menu" aria-expanded={speedOpen}
            disabled={!ready} onClick={() => setSpeedOpen(open => !open)}>{rate}×</button>
          {speedOpen && <span className="mv-speed-menu" role="menu" aria-label="Playback speed">{RATES.map(option => <button key={option} type="button" role="menuitemradio"
            aria-checked={option === rate} autoFocus={option === rate} onClick={() => { command('setPlaybackRate', [option]); setSpeedOpen(false); }}>{option}×</button>)}</span>}
        </span>
        <IconButton label={fullscreen ? 'Exit full screen' : 'Full screen'} shortcut="f" className="mv-icon-button mv-hide-mini" disabled={!ready} onClick={() => void changeFullscreen(!fullscreen)}>
          {fullscreen ? <Minimize size={17} /> : <Maximize size={17} />}</IconButton>
      </div>
      {failed && <p role="status">Video unavailable or slow to load. Try opening it on YouTube.</p>}
    </div>
  </div>;
}
