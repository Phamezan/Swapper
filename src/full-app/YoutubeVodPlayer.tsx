import { useEffect, useRef, useState, type ButtonHTMLAttributes, type CSSProperties, type KeyboardEvent, type ReactNode } from 'react';
import { Maximize, PanelRightOpen, SkipBack, SkipForward, X } from 'lucide-react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { isTauri } from '@tauri-apps/api/core';
import type { MatchupVod } from './matchupGuide';
import { adjacentMoment, currentMomentIndex, formatVodTime, groupMarkers, playerKeyAction, segmentOf, vodUrl } from './matchupVodModel';
import { VodMomentArt } from './VodMomentArt';
import { GuideSourceButton } from './GuideSourceButton';
import type { ChampionOption } from '../runes/types';
import { embedUrl, playerMessage, YOUTUBE_ORIGIN, type PlayerInfo } from './youtubePlayer';

type Moment = MatchupVod['moments'][number];
export type VodSeek = { moment: Moment; serial: number };

const PLAYING = 1;
// Ticks closer than this many pixels merge into one group so their hit areas never overlap.
const TICK_SPACING_PX = 16;
// After the browser leaves element fullscreen on Esc, the same key must not also close the VOD.
const ESC_GRACE_MS = 500;
// Closing the key-moment sidebar is a preference: it stays closed for every VOD, also after a restart.
const SIDEBAR_HIDDEN_KEY = 'swapper.vodMomentsHidden';
const sidebarHidden = () => { try { return localStorage.getItem(SIDEBAR_HIDDEN_KEY) === '1' || (localStorage.getItem(SIDEBAR_HIDDEN_KEY) === null && window.innerWidth < 1400); } catch { return window.innerWidth < 1400; } };

function IconButton({ label, shortcut, children, ...props }: { label: string; shortcut?: string } & ButtonHTMLAttributes<HTMLButtonElement>) {
  return <button type="button" className="mv-icon-button" aria-label={label} aria-keyshortcuts={shortcut} title={shortcut ? `${label} (${shortcut})` : label} {...props}>{children}</button>;
}

/** Key moments along the video's length: ticks to jump to, not a scrubber (YouTube's own bar scrubs). */
function MomentRail({ moments, current, duration, start = 0, disabled, onJump, champions }: {
  moments: readonly Moment[]; current: number; duration: number; start?: number; disabled: boolean; onJump: (sec: number) => void;
  champions: readonly ChampionOption[];
}) {
  const lane = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(480);
  useEffect(() => {
    const element = lane.current;
    if (!element || typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(([entry]) => setWidth(entry.contentRect.width || 480));
    observer.observe(element);
    return () => observer.disconnect();
  }, []);
  const percent = (sec: number) => `${Math.max(0, Math.min(100, (sec - start) / duration * 100))}%`;
  const currentIndex = currentMomentIndex(moments, current);
  const previous = adjacentMoment(moments, current, -1), next = adjacentMoment(moments, current, 1);
  return <>
    <IconButton label="Previous key moment" shortcut="[" disabled={disabled || !previous} onClick={() => previous && onJump(previous.atSec)}><SkipBack size={15} /></IconButton>
    <div className="mv-rail" ref={lane}>
      <span className="mv-rail-line"><span className="mv-rail-played" style={{ width: percent(current) }} /></span>
      {groupMarkers(moments, duration * TICK_SPACING_PX / width).map(group => {
        const members = group.indices.map(i => moments[i]);
        const edge = (group.atSec - start) / duration < .2 ? ' is-start' : (group.atSec - start) / duration > .8 ? ' is-end' : '';
        return <button key={group.indices[0]} type="button" disabled={disabled} data-kind={members[0].kind ?? 'none'}
          className={`mv-tick${group.atSec <= current ? ' is-past' : ''}${group.indices.includes(currentIndex) ? ' is-current' : ''}${edge}`} style={{ left: percent(group.atSec) }}
          aria-label={`Jump to ${members.map(moment => `${moment.label} at ${formatVodTime(moment.atSec)}`).join('; ')}`} onClick={() => onJump(group.atSec)}>
          {members.length > 1 && <span className="mv-tick-count">{members.length}</span>}
          <span className="mv-tip" aria-hidden>{members.map((moment, i) => <span key={i}>
            <VodMomentArt moment={moment} champions={champions} index={group.indices[i]} /><b>{formatVodTime(moment.atSec)}</b><span>{moment.label}</span>
          </span>)}</span>
        </button>;
      })}
    </div>
    <IconButton label="Next key moment" shortcut="]" disabled={disabled || !next} onClick={() => next && onJump(next.atSec)}><SkipForward size={15} /></IconButton>
  </>;
}

/** YouTube's embed with its own controls, sized 16:9 to the space, plus the League-specific parts: header, moment rail and key-moment sidebar (`children`). */
export function YoutubeVodPlayer({ vod, seek, mini = false, champions = [], onMomentChange, header, children, onNextGame }: {
  champions?: readonly ChampionOption[]; vod: MatchupVod; seek: VodSeek | null; mini?: boolean; onMomentChange?: (index: number) => void;
  onNextGame?: () => void; header?: ReactNode; children?: ReactNode;
}) {
  const frame = useRef<HTMLIFrameElement>(null);
  const root = useRef<HTMLDivElement>(null);
  const fullscreenNow = useRef(false);
  // Fullscreen changes of ours still in flight; their late fullscreenchange events are not user exits.
  const transitions = useRef(0);
  const fullscreenExitedAt = useRef(-Infinity);
  const restoreMaximized = useRef(false);
  const [info, setInfo] = useState<PlayerInfo>({});
  const [ready, setReady] = useState(false);
  const [failed, setFailed] = useState(false);
  const [fullscreen, setFullscreen] = useState(false);
  const [fullscreenError, setFullscreenError] = useState('');
  const [collapsed, setCollapsed] = useState(sidebarHidden);
  const [segmentDone, setSegmentDone] = useState(false);
  const [released, setReleased] = useState(false);
  const segment = segmentOf(vod);
  const hasSegment = vod.startSec !== undefined || vod.endSec !== undefined;
  const src = embedUrl(vod.videoId, location.origin, hasSegment ? { start: segment.start } : undefined);
  fullscreenNow.current = fullscreen;
  const playing = info.playerState === PLAYING;
  const duration = info.duration || vod.durationSec;
  const segStart = segment.start, segEnd = vod.endSec ?? duration, segDuration = Math.max(segEnd - segStart, 0);
  const current = Math.min(info.currentTime ?? 0, duration);
  const momentIndex = currentMomentIndex(vod.moments, current);
  const send = (message: object) => frame.current?.contentWindow?.postMessage(JSON.stringify(message), YOUTUBE_ORIGIN);
  const command = (func: string, args: unknown[] = []) => send({ event: 'command', func, args });
  // Seek optimistically: YouTube reports progress a few times a second, so the rail would lag behind the click.
  const seekTo = (sec: number) => {
    const to = released ? Math.max(0, Math.min(sec, duration)) : Math.max(segStart, Math.min(sec, segEnd));
    command('seekTo', [to, true]);
    setInfo(previous => ({ ...previous, currentTime: to }));
  };
  const jump = (sec: number) => { seekTo(sec); command('playVideo'); };
  // Resize the native window before WebView2 captures the element-fullscreen viewport.
  // Browsers still enter synchronously; desktop has the popover fallback if activation expires.
  // tao clamps a maximized borderless window to the monitor work area, which left a taskbar-high band under fullscreen.
  const nativeFullscreen = async (on: boolean) => {
    const native = getCurrentWindow();
    if (on) {
      restoreMaximized.current = await native.isMaximized();
      if (restoreMaximized.current) await native.toggleMaximize();
    }
    await native.setFullscreen(on);
    if (!on && restoreMaximized.current) { restoreMaximized.current = false; await native.toggleMaximize(); }
  };
  const changeFullscreen = async (on: boolean) => {
    transitions.current++;
    try { await applyFullscreen(on); } finally { transitions.current--; }
  };
  const applyFullscreen = async (on: boolean) => {
    fullscreenNow.current = on;
    if (on) setFullscreen(true);
    setFullscreenError('');
    if (on && isTauri()) {
      try { await nativeFullscreen(true); }
      catch (error) { setFullscreenError(`Window fullscreen unavailable: ${String(error)}`); }
      if (!fullscreenNow.current) return;
    }
    try {
      if (on && !document.fullscreenElement) await root.current?.requestFullscreen();
      if (!on && document.fullscreenElement) await document.exitFullscreen();
    } catch { /* The Tauri window can still enter fullscreen when the element API is refused. */ }
    // A manual popover supplies the top layer without moving/remounting the iframe.
    try {
      if (on && !document.fullscreenElement) root.current?.showPopover?.();
      if (!on) root.current?.hidePopover?.();
    } catch { /* A detached view must not prevent the window from leaving fullscreen. */ }
    if (!on && isTauri()) {
      try { await nativeFullscreen(false); }
      catch (error) { setFullscreenError(`Could not leave window fullscreen: ${String(error)}`); }
    }
    if (!on) setFullscreen(false);
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
  }, [vod.videoId, vod.startSec, vod.endSec, vod.fixture]);
  useEffect(() => { if (ready && seek) jump(seek.moment.atSec); }, [ready, seek]);
  useEffect(() => {
    const time = info.currentTime;
    if (time === undefined || !hasSegment || released) return;
    if (vod.endSec !== undefined && time >= segEnd) { if (!segmentDone || playing) command('pauseVideo'); setSegmentDone(true); if (fullscreenNow.current) void changeFullscreen(false); }
    else if (time < segStart - 1) seekTo(segStart);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [info.currentTime, hasSegment, segStart, segEnd, vod.endSec, released, playing]);
  useEffect(() => { onMomentChange?.(momentIndex); }, [momentIndex, onMomentChange]);
  useEffect(() => {
    if (!fullscreen) return;
    const sync = () => {
      // Our own exit's event can land after a quick re-entry; only an exit we did not start (Esc, F11) counts.
      if (document.fullscreenElement || transitions.current) return;
      fullscreenExitedAt.current = performance.now();
      void changeFullscreen(false);
    };
    document.addEventListener('fullscreenchange', sync);
    return () => document.removeEventListener('fullscreenchange', sync);
  }, [fullscreen]);
  useEffect(() => { if (mini && fullscreen) void changeFullscreen(false); }, [mini, fullscreen]);
  useEffect(() => () => { if (fullscreenNow.current) void nativeFullscreen(false).catch(() => {}); }, []);
  const previous = adjacentMoment(vod.moments, current, -1), next = adjacentMoment(vod.moments, current, 1);
  // Keys reach us only while focus is on our UI; when the video has focus, YouTube handles its own.
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === 'Escape' && (fullscreen || performance.now() - fullscreenExitedAt.current < ESC_GRACE_MS)) {
      event.preventDefault(); event.stopPropagation();
      if (fullscreen) void changeFullscreen(false);
      return;
    }
    const action = ready && playerKeyAction({ key: event.key, ctrlKey: event.ctrlKey, metaKey: event.metaKey, altKey: event.altKey, target: event.target as HTMLElement });
    if (!action || (action === 'fullscreen' && mini)) return;
    event.preventDefault();
    if (action === 'toggle') command(playing ? 'pauseVideo' : 'playVideo');
    else if (action === 'mute') { command(info.muted ? 'unMute' : 'mute'); setInfo(previous => ({ ...previous, muted: !previous.muted })); }
    else if (action === 'fullscreen') void changeFullscreen(!fullscreen);
    else if ('seekBy' in action) seekTo(current + action.seekBy);
    else { const target = action.moment > 0 ? next : previous; if (target) jump(target.atSec); }
  };
  const url = vodUrl(vod);
  const notice = failed ? 'Video unavailable or slow to load.' : fullscreenError;
  // The rail needs the API connection; a failed embed gets the notice instead.
  const hasMoments = !fullscreen && !mini && vod.moments.length > 0, hasRail = hasMoments && !failed;
  const showSidebar = hasMoments && !!children && !collapsed;
  const toggleSidebar = () => {
    try { localStorage.setItem(SIDEBAR_HIDDEN_KEY, collapsed ? '0' : '1'); } catch { /* Private storage only loses the preference. */ }
    setCollapsed(!collapsed);
  };
  // One bar under the picture, like a media player's control strip: key moments on the left, fullscreen bottom right.
  const showBar = !fullscreen && !mini && !!url;
  const fullscreenButton = <IconButton label={fullscreen ? 'Exit full screen' : 'Full screen'} shortcut="f" className="mv-icon-button mv-strong" onClick={() => void changeFullscreen(!fullscreen)}>
    <Maximize size={16} /></IconButton>;
  return <div ref={root} popover="manual" tabIndex={-1} onKeyDown={onKeyDown}
    className={`mv-video-player${fullscreen ? ' is-fullscreen' : ''}`}
    style={{ '--mv-bars': fullscreen ? 0 : Number(showBar) + Number(!!notice) + Number(segmentDone && !released) } as CSSProperties}>
    {!mini && !fullscreen && <header className="mv-watch-header">{header}</header>}
    <div className="mv-stage"><div className="mv-screen">
      {url ? <div className="mv-frame mv-embed"><iframe ref={frame} title={vod.title} src={src}
        allow="autoplay; encrypted-media; picture-in-picture"
        onLoad={() => { setFailed(false); send({ event: 'listening', id: 1 }); }} /></div>
        : <div className="mv-frame mv-preview-frame"><span className="mv-preview">Preview, no video</span><p>Preview only · no video linked</p></div>}
      {!fullscreen && segmentDone && !released && <div className="mv-game-over" role="status"><strong>Game over</strong>{onNextGame && <button onClick={onNextGame}>Next game</button>}<button onClick={() => { setReleased(true); setSegmentDone(false); command('playVideo'); }}>Keep watching</button></div>}
      {!fullscreen && notice && <div className="mv-notice" role="status"><span>{notice}</span>
        {failed && url && <GuideSourceButton url={url} className="mv-youtube-link" label="Open on YouTube">Open on YouTube ↗</GuideSourceButton>}</div>}
      {showBar && <div className="mv-rail-row" role="group" aria-label="Video tools">
        {hasRail ? <MomentRail moments={vod.moments} current={current} duration={segDuration || duration} start={segStart} disabled={!ready} onJump={jump} champions={champions} /> : <span className="mv-rail-spacer" />}
        <span className="mv-bar-end">
          {hasMoments && collapsed && <IconButton label="Show key moments" className="mv-show-moments" onClick={toggleSidebar}><PanelRightOpen size={15} aria-hidden />Key moments</IconButton>}
          {fullscreenButton}
        </span>
      </div>}
    </div></div>
    {showSidebar && <div className="mv-side">
      <span className="mv-side-close"><IconButton label="Hide key moments" onClick={toggleSidebar}><X size={15} /></IconButton></span>
      {children}
    </div>}
  </div>;
}
