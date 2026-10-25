import { pageLabel } from "./ui/pageNames";
import { useEffect, useMemo, useRef, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { getVersion } from "@tauri-apps/api/app";
import { Copy, Minus, Square, X } from "lucide-react";
import { AccountsIcon, DashboardIcon, DraftIcon, LiveGameIcon, MatchupsIcon, SettingsIcon, TierlistIcon, BackIcon, ForwardIcon, RefreshIcon, SearchIcon, SwitchIcon } from "./ui/icons";

import { ChampionIcon } from "../runes/ChampionIcon";
import type { ChampionOption } from "../runes/types";
import { ProfileIcon } from './ProfileIcon';
import { RECENT_KEY, readRecents, remember, savedPlayers, rankPlayers, type PlayerHit, type PlayerSearch, type Recent } from './playerModel';
import type { Snapshot } from './types';
import { fmtAgo } from '../runes/types';
import type { LocationSegment } from './locationTrail';
import './search.css';
import { UpdatePill } from '../settings/UpdateSection';

type Props = {
  trail: LocationSegment[];
  onPickLocation: (page: string) => void;
  canBack: boolean;
  canForward: boolean;
  onBack: () => void;
  onForward: () => void;
  onRefresh: () => void;
  refreshing: boolean;
  champions: ChampionOption[];
  pages: string[];
  onPickChampion: (champion: ChampionOption) => void;
  onPickPage: (page: string) => void;
  onPickPlayer: (riotId: string) => void;
  snapshot?: Snapshot | null;
  onRestart: () => void;
};

type Result = ({ kind: "champion"; champion: ChampionOption } | { kind: "page"; page: string } | { kind: 'action' } | { kind: 'player'; player: PlayerHit }) & { group: string };
const pageIcons: Record<string, typeof DashboardIcon> = { Dashboard: DashboardIcon, 'Live Game': LiveGameIcon, 'Tier list & Builds': TierlistIcon, Matchups: MatchupsIcon, 'Champion select': DraftIcon, Accounts: AccountsIcon, Settings: SettingsIcon };

/** Frameless window chrome: drag region, history, Ctrl+K search and window buttons. */
export function TitleBar(props: Props) {
  const [open, setOpen] = useState(false);
  const searchButton = useRef<HTMLButtonElement>(null);
  const returnFocus = useRef(false);
  useEffect(() => {
    if (!open && returnFocus.current) { returnFocus.current = false; searchButton.current?.focus(); }
  }, [open]);
  const closeSearch = (restoreFocus = false) => { returnFocus.current = restoreFocus; setOpen(false); };
  const [maximized, setMaximized] = useState(false);
  const native = isTauri();
  const [version, setVersion] = useState<string | null>(null);
  useEffect(() => { if (native) void getVersion().then(setVersion).catch(() => {}); }, [native]);
  useEffect(() => {
    const key = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") { event.preventDefault(); setOpen(true); }
      if (event.altKey && event.key === "ArrowLeft" && props.canBack) { event.preventDefault(); setOpen(false); props.onBack(); }
      if (event.altKey && event.key === "ArrowRight" && props.canForward) { event.preventDefault(); setOpen(false); props.onForward(); }
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [props.canBack, props.canForward, props.onBack, props.onForward]);
  useEffect(() => {
    if (!native) return;
    const current = getCurrentWindow();
    const sync = () => void current.isMaximized().then(setMaximized).catch(() => {});
    sync();
    const stop = current.onResized(sync);
    return () => { void stop.then(unlisten => unlisten()); };
  }, [native]);
  const win = native ? getCurrentWindow() : null;
  return <header className="title-bar" data-tauri-drag-region>
    <div className="title-brand" data-tauri-drag-region><SwitchIcon size={30} /><span>Swapper</span>{version && <small className="title-version">v{version}</small>}</div>
    <div className="title-nav">
      <button className="title-icon" aria-label="Back" title="Back (Alt+←)" disabled={!props.canBack} onClick={props.onBack}><BackIcon size={16} /></button>
      <button className="title-icon" aria-label="Forward" title="Forward (Alt+→)" disabled={!props.canForward} onClick={props.onForward}><ForwardIcon size={16} /></button>
    </div>
    <nav className={`title-location${open ? ' is-searching' : ''}`} aria-label="Location">
      {open ? <Palette {...props} onClose={closeSearch} /> : <div className="location-view" title={props.trail.map(segment => segment.label).join(' › ')}>
        <button ref={searchButton} className="location-search-button" onClick={() => setOpen(true)} aria-label="Search (Ctrl+K)" aria-keyshortcuts="Control+k Meta+k"><kbd>Ctrl K</kbd></button>
        <ol className="location-trail">{props.trail.map((segment, index) => <li key={`${index}-${segment.label}`}>
          {index > 0 && <span className="location-separator" aria-hidden>›</span>}
          {segment.page ? <button className="location-segment" onClick={() => props.onPickLocation(segment.page!)}>{segment.label}</button>
            : <span className="location-segment" aria-current="page">{segment.label}</span>}
        </li>)}</ol>
        <button className="title-icon location-refresh" aria-label="Refresh stats" title="Refresh stats" disabled={props.refreshing} onClick={props.onRefresh}><RefreshIcon size={14} className={props.refreshing ? "spin" : undefined} /></button>
      </div>}
    </nav>
    <div className="title-spacer" data-tauri-drag-region />
    <UpdatePill />
    {win && <div className="window-buttons">
      <button aria-label="Minimize" onClick={() => void win.minimize()}><Minus size={15} /></button>
      <button aria-label={maximized ? "Restore" : "Maximize"} onClick={() => void win.toggleMaximize()}>{maximized ? <Copy size={12} /> : <Square size={12} />}</button>
      <button className="window-close" aria-label="Close" onClick={() => void win.close()}><X size={16} /></button>
    </div>}
  </header>;
}

const RESTART_ACTION = "Restart client";

function Palette({ champions, pages, snapshot, onPickChampion, onPickPage, onPickPlayer, onRestart, onClose }: Props & { onClose: (restoreFocus?: boolean) => void }) {
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  const dialog = useRef<HTMLDivElement>(null);
  const [recents, setRecents] = useState<Recent[]>(() => { try { return readRecents(localStorage.getItem(RECENT_KEY)); } catch { return []; } });
  const [lookup, setLookup] = useState<(PlayerSearch & { query: string }) | null>(null);
  const [localLookup, setLocalLookup] = useState<(PlayerSearch & { query: string }) | null>(null);
  const [pending, setPending] = useState(false);
  const q = query.trim();
  useEffect(() => {
    input.current?.focus();
    const outside = (event: PointerEvent) => { if (!dialog.current?.contains(event.target as Node)) onClose(); };
    document.addEventListener('pointerdown', outside);
    return () => document.removeEventListener('pointerdown', outside);
  }, []);
  useEffect(() => {
    let live = true;
    setPending(Boolean(q));
    invoke<PlayerSearch>('full_app_search_players', { query: q, online: false })
      .then(result => { if (live) setLocalLookup({ ...result, query: q }); }).catch(() => {});
    if (q.length < 2) { setPending(false); return () => { live = false; }; }
    const timer = setTimeout(() => {
      invoke<PlayerSearch>('full_app_search_players', { query: q, online: true }).then(result => { if (live) setLookup({ ...result, query: q }); })
        .catch(() => { if (live) setLookup({ connected: false, players: [], message: null, query: q }); })
        .finally(() => { if (live) setPending(false); });
    }, 250);
    return () => { live = false; clearTimeout(timer); };
  }, [q]);
  useEffect(() => () => { void invoke('full_app_search_players', { query: '', online: false }).catch(() => {}); }, []);
  const playerHits = useMemo(() => {
    if (!q) return [];
    const saved = savedPlayers(snapshot, q);
    const remote = lookup?.query === q ? lookup.players : [];
    const local = localLookup?.query === q ? localLookup.players : [];
    const recent = recents.filter((r): r is Extract<Recent, { kind: 'player' }> => r.kind === 'player')
      .map(r => ({ riotId: r.riotId, icon: null, level: null, ally: null, lastPlayed: null }));
    return rankPlayers([...remote, ...local, ...saved, ...recent], q);
  }, [snapshot, q, lookup, localLookup, recents]);
  const results = useMemo<Result[]>(() => {
    const q = query.trim().toLowerCase();
    const pageHits = pages.filter(page => !q || pageLabel(page).toLowerCase().includes(q) || page.toLowerCase().includes(q)).map(page => ({ kind: "page", page, group: 'Pages' }) as Result);
    const actionHits = !q || RESTART_ACTION.toLowerCase().includes(q) || 'restart riot client'.includes(q) ? [{ kind: 'action', group: 'Actions' } as Result] : [];
    const championHits = q ? champions
      .filter(c => c.name.toLowerCase().includes(q))
      .sort((a, b) => Number(!a.name.toLowerCase().startsWith(q)) - Number(!b.name.toLowerCase().startsWith(q)))
      .slice(0, 8)
      .map(champion => ({ kind: "champion", champion, group: 'Champions' }) as Result) : [];
    const recentHits: Result[] = q ? [] : recents.map(r => r.kind === 'champion' ? { ...r, group: 'Recent' }
      : { kind: 'player', player: localLookup?.query === '' ? localLookup.players.find(p => p.riotId.toLowerCase() === r.riotId.toLowerCase()) ?? { riotId: r.riotId, icon: null, level: null, ally: null, lastPlayed: null }
        : { riotId: r.riotId, icon: null, level: null, ally: null, lastPlayed: null }, group: 'Recent' });
    return [...recentHits, ...pageHits, ...actionHits, ...championHits, ...playerHits.map(player => ({ kind: 'player', player, group: 'Players' } as Result))];
  }, [query, champions, pages, playerHits, recents, localLookup]);
  useEffect(() => setActive(0), [query, results.length]);
  useEffect(() => { document.getElementById(`palette-${active}`)?.scrollIntoView({ block: 'nearest' }); }, [active]);
  function save(next: Recent[]) { setRecents(next); try { localStorage.setItem(RECENT_KEY, JSON.stringify(next)); } catch { /* Storage can be disabled; search still works. */ } }
  const pick = (result: Result | undefined) => {
    if (!result) return;
    if (result.kind === "champion") { save(remember(recents, { kind: 'champion', champion: result.champion })); onPickChampion(result.champion); }
    else if (result.kind === 'player') { save(remember(recents, { kind: 'player', riotId: result.player.riotId })); onPickPlayer(result.player.riotId); }
    else if (result.kind === 'action') onRestart();
    else onPickPage(result.page);
    onClose(true);
  };
  return <div ref={dialog} className="location-search" onBlur={event => {
      if (!event.currentTarget.contains(event.relatedTarget as Node | null)) onClose();
    }} onKeyDown={event => {
      if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); onClose(true); }
    }}>
      <label className="palette-input"><SearchIcon size={16} aria-hidden />
        <input ref={input} value={query} maxLength={128} placeholder="Search champions, players, pages…" aria-label="Search champions, pages and players"
          role="combobox" aria-autocomplete="list" autoComplete="off" spellCheck={false} aria-expanded aria-controls="palette-results" aria-activedescendant={results[active] ? `palette-${active}` : undefined}
          onChange={event => setQuery(event.target.value)}
          onKeyDown={event => {
            if (event.key === "ArrowDown") { event.preventDefault(); setActive(i => Math.max(0, Math.min(i + 1, results.length - 1))); }
            else if (event.key === "ArrowUp") { event.preventDefault(); setActive(i => Math.max(i - 1, 0)); }
            else if (event.key === "Enter") { event.preventDefault(); pick(results[active]); }
          }} />
      </label>
      <div className="palette">
      <div className="palette-hint"><span>{q ? pending ? 'Searching players…' : 'Player suggestions via OP.GG' : 'Jump to a page, or type to search'}</span>
        {recents.length > 0 && <button onClick={() => { save([]); input.current?.focus(); }}>Clear recent</button>}</div>
      <div id="palette-results" role="listbox" aria-label="Search results">
        {['Recent', 'Pages', 'Actions', 'Champions', 'Players'].map(group => {
          const hits = results.map((result, index) => ({ result, index })).filter(({ result }) => result.group === group);
          return hits.length > 0 && <div role="group" aria-label={group} key={group}><h3 aria-hidden="true">{group}</h3>
            {hits.map(({ result, index: i }) => {
              const Icon = result.kind === 'page' ? pageIcons[result.page] ?? DashboardIcon : result.kind === 'action' ? RefreshIcon : AccountsIcon;
              return <div className="palette-option" key={result.kind === 'page' ? result.page : result.kind === 'action' ? RESTART_ACTION : result.kind === 'champion' ? result.champion.id : result.player.riotId} id={`palette-${i}`} role="option" aria-selected={i === active}
                onPointerDown={event => event.preventDefault()}
                onPointerEnter={() => setActive(i)} onClick={() => pick(result)}>
                {result.kind === 'champion' ? <><ChampionIcon id={result.champion.id} name={result.champion.name} mode="desktop" /><span>{result.champion.name}</span><small>Build and counters</small></>
                  : result.kind === 'action' ? <><span className="palette-nav-icon"><Icon size={19} aria-hidden /></span><span>{RESTART_ACTION}</span></>
                  : result.kind === 'page' ? <><span className="palette-nav-icon"><Icon size={19} aria-hidden /></span><span>{pageLabel(result.page)}</span><small>Page</small></>
                  : <><ProfileIcon player={result.player} /><span className="palette-player-copy">{result.player.riotId}<small>{result.player.lastPlayed && result.player.ally != null ? `${result.player.ally ? 'Played with you' : 'Played against you'} · ${fmtAgo(result.player.lastPlayed)}` : group === 'Recent' ? 'Recent search' : 'Player profile'}</small></span>{result.player.live && <span className="nav-badge live"><i aria-hidden />LIVE</span>}{result.player.level != null && <small>Level {result.player.level}</small>}</>}
              </div>;
            })}</div>;
        })}
      </div>
      {!results.length && <div className="palette-empty" role="status"><SearchIcon size={24} /><strong>{pending ? 'Searching…' : 'No results found'}</strong><span>Try a champion, page or a full Riot ID: Name#TAG.</span></div>}
      {q && <p className="palette-player-status" role="status">{lookup?.query === q && lookup.message ? lookup.message : 'Search by name, or use Name#TAG for an exact League Client lookup.'}</p>}
      </div>
  </div>;
}
