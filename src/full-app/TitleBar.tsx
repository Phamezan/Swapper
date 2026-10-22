import { pageLabel } from "./ui/pageNames";
import { useEffect, useMemo, useRef, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Copy, Minus, Square, X } from "lucide-react";
import { BackIcon, ForwardIcon, RefreshIcon, SearchIcon } from "./ui/icons";
import { BrandIcon } from "../components/BrandIcon";
import { ChampionIcon } from "../runes/ChampionIcon";
import type { ChampionOption } from "../runes/types";

type Props = {
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
};

type Result = { kind: "champion"; champion: ChampionOption } | { kind: "page"; page: string };

/** Frameless window chrome: drag region, history, Ctrl+K search and window buttons. */
export function TitleBar(props: Props) {
  const [open, setOpen] = useState(false);
  const [maximized, setMaximized] = useState(false);
  const native = isTauri();
  useEffect(() => {
    const key = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") { event.preventDefault(); setOpen(true); }
      if (event.altKey && event.key === "ArrowLeft" && props.canBack) props.onBack();
      if (event.altKey && event.key === "ArrowRight" && props.canForward) props.onForward();
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
    <div className="title-brand" data-tauri-drag-region><BrandIcon size={20} /><span data-tauri-drag-region>Swapper</span></div>
    <div className="title-nav">
      <button className="title-icon" aria-label="Back" title="Back (Alt+←)" disabled={!props.canBack} onClick={props.onBack}><BackIcon size={16} /></button>
      <button className="title-icon" aria-label="Forward" title="Forward (Alt+→)" disabled={!props.canForward} onClick={props.onForward}><ForwardIcon size={16} /></button>
      <button className="title-icon" aria-label="Refresh stats" title="Refresh stats" disabled={props.refreshing} onClick={props.onRefresh}><RefreshIcon size={14} className={props.refreshing ? "spin" : undefined} /></button>
    </div>
    <button className="title-search" onClick={() => setOpen(true)} aria-label="Search champions and pages">
      <SearchIcon size={14} aria-hidden /><span>Search champions and pages</span><kbd>Ctrl</kbd><kbd>K</kbd>
    </button>
    <div className="title-spacer" data-tauri-drag-region />
    {win && <div className="window-buttons">
      <button aria-label="Minimize" onClick={() => void win.minimize()}><Minus size={15} /></button>
      <button aria-label={maximized ? "Restore" : "Maximize"} onClick={() => void win.toggleMaximize()}>{maximized ? <Copy size={12} /> : <Square size={12} />}</button>
      <button className="window-close" aria-label="Close" onClick={() => void win.close()}><X size={16} /></button>
    </div>}
    {open && <Palette {...props} onClose={() => setOpen(false)} />}
  </header>;
}

function Palette({ champions, pages, onPickChampion, onPickPage, onClose }: Props & { onClose: () => void }) {
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => { input.current?.focus(); }, []);
  const results = useMemo<Result[]>(() => {
    const q = query.trim().toLowerCase();
    const pageHits = pages.filter(page => !q || pageLabel(page).toLowerCase().includes(q) || page.toLowerCase().includes(q)).map(page => ({ kind: "page", page }) as Result);
    const championHits = q ? champions
      .filter(c => c.name.toLowerCase().includes(q))
      .sort((a, b) => Number(!a.name.toLowerCase().startsWith(q)) - Number(!b.name.toLowerCase().startsWith(q)))
      .slice(0, 8)
      .map(champion => ({ kind: "champion", champion }) as Result) : [];
    return [...championHits, ...pageHits];
  }, [query, champions, pages]);
  useEffect(() => setActive(0), [query]);
  const pick = (result: Result | undefined) => {
    if (!result) return;
    if (result.kind === "champion") onPickChampion(result.champion); else onPickPage(result.page);
    onClose();
  };
  return <div className="palette-backdrop" onPointerDown={event => { if (event.target === event.currentTarget) onClose(); }}>
    <div className="palette" role="dialog" aria-label="Search">
      <label className="palette-input"><SearchIcon size={16} aria-hidden />
        <input ref={input} value={query} placeholder="Champion or page…" aria-label="Search champions and pages"
          role="combobox" aria-expanded aria-controls="palette-results" aria-activedescendant={results[active] ? `palette-${active}` : undefined}
          onChange={event => setQuery(event.target.value)}
          onKeyDown={event => {
            if (event.key === "Escape") onClose();
            else if (event.key === "ArrowDown") { event.preventDefault(); setActive(i => Math.min(i + 1, results.length - 1)); }
            else if (event.key === "ArrowUp") { event.preventDefault(); setActive(i => Math.max(i - 1, 0)); }
            else if (event.key === "Enter") pick(results[active]);
          }} />
      </label>
      <ul id="palette-results" role="listbox">
        {results.map((result, i) => <li key={result.kind === "page" ? `p-${result.page}` : `c-${result.champion.id}`} id={`palette-${i}`} role="option" aria-selected={i === active}
          onPointerEnter={() => setActive(i)} onClick={() => pick(result)}>
          {result.kind === "champion"
            ? <><ChampionIcon id={result.champion.id} name={result.champion.name} mode="desktop" /><span>{result.champion.name}</span><small>Build and counters</small></>
            : <><span className="palette-page-dot" aria-hidden /><span>{pageLabel(result.page)}</span><small>Page</small></>}
        </li>)}
        {!results.length && <li className="palette-empty">No champion or page matches “{query}”.</li>}
      </ul>
    </div>
  </div>;
}
