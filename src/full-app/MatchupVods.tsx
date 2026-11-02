import { useEffect, useRef, useState } from "react";
import { ArrowLeft, ArrowUpRight, ChevronRight, X } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { VodMomentArt } from "./VodMomentArt";
import type { ChampionOption } from "../runes/types";
import { ChampionIcon } from "../runes/ChampionIcon";
import { RankCrest } from "../runes/RankCrest";
import { GuideSourceButton } from "./GuideSourceButton";
import { loadChampionVods, loadMatchupVods } from "./loadMatchupVods";
import { GUIDE_SECTIONS, type GuideSectionId, type MatchupVod } from "./matchupGuide";
import { ANY_VOD, comparePatch, filterVods, formatVodTime, RANK_TIERS, rankTier, vodMatches, vodPlayerName, vodUrl, type RankTier, type VodFilters, type VodSort } from "./matchupVodModel";
import { NearEnd } from "./NearViewport";
import { Picker, ROLES } from "./Picker";
import { VodPlayer } from "./VodPlayer";
import { YoutubeVodPlayer, type VodSeek } from "./YoutubeVodPlayer";
import { Skeleton } from "./ui/States";
import "./matchup-vods.css";

// Rows render in steps as the list scrolls, so a 1,000-VOD library opens as fast as a short one.
const ROW_STEP = 60;
const LANES: Record<string, string> = Object.fromEntries(ROLES);
const SORTS: [VodSort, string][] = [["patch", "Newest patch"], ["shortest", "Shortest first"], ["longest", "Longest first"]];
type Scope = "matchup" | "champion";

function Segment<T>({ label, value, options, onChange }: { label: string; value: T; options: [T, string, number | null][]; onChange: (value: T) => void }) {
  return <div className="segmented mv-segment" role="group" aria-label={label}>{options.map(([option, text, count]) =>
    <button key={text} type="button" className={option === value ? "is-on" : undefined} aria-pressed={option === value} onClick={() => onChange(option)}>{text}{count !== null && <small>{count}</small>}</button>)}</div>;
}

function VodFacts({ vod }: { vod: MatchupVod }) {
  return <span className="mv-facts">{vod.result && <span className={`mv-result is-${vod.result === "W" ? "win" : "loss"}`} aria-label={vod.result === "W" ? "Win" : "Loss"}>{vod.result === "W" ? "Victory" : "Defeat"}</span>}
    <span>Patch {vod.patch}</span><span>{formatVodTime(vod.durationSec)}</span><span>{vod.moments.length} key moments</span>
  </span>;
}

export function MatchupVods({ champion, enemy, lane, onGuideSection, active = true, onReopen, onPlaybackChange }: {
  champion: ChampionOption; enemy: ChampionOption; lane: string; onGuideSection: (section: GuideSectionId) => void;
  active?: boolean; onReopen?: () => void;
  onPlaybackChange?: (open: boolean) => void;
}) {
  const [response, setResponse] = useState<{ key: string; vods: MatchupVod[] } | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [seek, setSeek] = useState<VodSeek | null>(null);
  const [activeMoment, setActiveMoment] = useState(-1);
  const [champions, setChampions] = useState<ChampionOption[]>([]);
  const [scope, setScope] = useState<Scope>("matchup");
  const [all, setAll] = useState<MatchupVod[] | null>(null);
  const [filters, setFilters] = useState<VodFilters>(ANY_VOD);
  const [limit, setLimit] = useState(ROW_STEP);
  useEffect(() => {
    let live = true;
    void invoke<ChampionOption[]>("get_champion_list").then(list => { if (live) setChampions(list); }).catch(() => {});
    return () => { live = false; };
  }, []);
  const momentList = useRef<HTMLOListElement>(null);
  const lastOpened = useRef("");
  const sourceRow = useRef<HTMLButtonElement>(null);
  const backButton = useRef<HTMLButtonElement>(null);
  const key = `${champion.id}-${enemy.id}-${lane}`;
  useEffect(() => {
    let live = true;
    void loadMatchupVods(champion.id, enemy.id, lane).then(vods => { if (live) setResponse({ key, vods }); })
      .catch(() => { if (live) setResponse({ key, vods: [] }); });
    return () => { live = false; };
  }, [champion.id, enemy.id, lane, key]);
  useEffect(() => {
    if (scope !== "champion" || all) return;
    let live = true;
    void loadChampionVods(champion.id).then(list => { if (live) setAll(list); }).catch(() => { if (live) setAll([]); });
    return () => { live = false; };
  }, [scope, all, champion.id]);
  useEffect(() => {
    if (!active) return;
    if (selectedId) backButton.current?.focus({ preventScroll: true });
    else sourceRow.current?.focus();
  }, [selectedId, active]);
  const vods = response?.key === key ? response.vods : null;
  const library = scope === "champion" ? all : vods;
  const selected = library?.find(vod => vod.videoId === selectedId);
  const playable = !!selected && !!vodUrl(selected);
  // Keep the playing moment in view by scrolling the feed only, never the page around the video.
  useEffect(() => {
    const list = momentList.current, row = list?.children[activeMoment] as HTMLElement | undefined;
    if (!list || !row || list.scrollHeight <= list.clientHeight) return;
    if (row.offsetTop < list.scrollTop || row.offsetTop + row.offsetHeight > list.scrollTop + list.clientHeight)
      list.scrollTo({ top: row.offsetTop - (list.clientHeight - row.offsetHeight) / 2, behavior: "smooth" });
  }, [activeMoment]);
  useEffect(() => {
    onPlaybackChange?.(playable);
    return () => onPlaybackChange?.(false);
  }, [playable, onPlaybackChange]);
  if (!active && !selectedId) return null;
  if (!vods) return <Skeleton label="Loading matchup VODs" rows={2} height={70} />;
  if (!selected) {
    const pool = library ?? [];
    const filter = (next: Partial<VodFilters>) => { setFilters(previous => ({ ...previous, ...next })); setLimit(ROW_STEP); };
    const count = (next: Partial<VodFilters>) => pool.filter(vod => vodMatches(vod, { ...filters, ...next })).length;
    const shown = filterVods(pool, filters);
    const patches = [...new Set(pool.map(vod => vod.patch))].sort((a, b) => comparePatch(b, a));
    const enemyName = (id?: number) => champions.find(c => c.id === id)?.name ?? "Opponent";
    const enemies = [...new Set(pool.flatMap(vod => vod.enemy ? [vod.enemy] : []))].map(id => ({ id, games: count({ enemy: id }), name: enemyName(id) }))
      .sort((a, b) => b.games - a.games || a.name.localeCompare(b.name));
    const wide = scope === "champion";
    return <div className="mv-library">
      <header className="mv-list-toolbar"><div><span className="mv-eyebrow">Match library</span><h2>{wide ? `Every ${champion.name} VOD.` : "Watch the matchup."}</h2>
        <p>{library ? `${pool.length} ${pool.length === 1 ? "game" : "games"} as ${champion.name} against ${wide ? `${enemies.length} ${enemies.length === 1 ? "opponent" : "opponents"}` : enemy.name}` : "Loading…"}</p></div>
        <div className="mv-scope">{pool.some(vod => vod.fixture) && <span className="mv-preview">Preview data</span>}
          <Segment<Scope> label="VOD scope" value={scope} onChange={next => { setScope(next); filter({ enemy: null }); }}
            options={[["matchup", `vs ${enemy.name}`, vods.length], ["champion", `All ${champion.name}`, all?.length ?? null]]} /></div></header>
      {!library ? <Skeleton label={`Loading every ${champion.name} VOD`} rows={6} height={40} />
        : !pool.length ? <div className="mv-empty"><p>{wide ? `No ${champion.name} VODs yet` : "No VODs for this matchup yet"}</p>
          {!wide && <button type="button" className="mv-text-button" onClick={() => setScope("champion")}>Browse every {champion.name} VOD<ChevronRight size={14} aria-hidden /></button>}</div>
        : <>
          <div className="mv-filters" role="group" aria-label="Filter VODs">
            <Segment label="Players" value={filters.pros} onChange={pros => filter({ pros })} options={[[false, "Everyone", count({ pros: false })], [true, "Pros", count({ pros: true })]]} />
            <Segment<VodFilters["result"]> label="Result" value={filters.result} onChange={result => filter({ result })} options={[[null, "Any result", count({ result: null })], ["W", "Wins", count({ result: "W" })], ["L", "Losses", count({ result: "L" })]]} />
            <Picker label="Rank" value={filters.tier ?? ""} onChange={tier => filter({ tier: (tier || null) as RankTier | null })} options={[{ value: "", label: `Any rank · ${count({ tier: null })}` },
              ...RANK_TIERS.map(([value, label]) => { const games = count({ tier: value }); return { value, label: `${label} · ${games}`, disabled: !games, icon: <RankCrest tier={value} size={22} mode="desktop" /> }; })]} />
            <Picker label="Patch" value={filters.patch ?? ""} onChange={patch => filter({ patch: patch || null })} options={[{ value: "", label: `Any patch · ${count({ patch: null })}` },
              ...patches.map(patch => { const games = count({ patch }); return { value: patch, label: `Patch ${patch} · ${games}`, disabled: !games }; })]} />
            {wide && <Picker label="Opponent" searchable value={String(filters.enemy ?? 0)} onChange={id => filter({ enemy: Number(id) || null })} options={[{ value: "0", label: `Any opponent · ${count({ enemy: null })}` },
              ...enemies.map(({ id, name, games }) => ({ value: String(id), label: `${name} · ${games}`, disabled: !games, icon: <ChampionIcon id={id} name={name} mode="desktop" /> }))]} />}
            <Picker label="Sort" value={filters.sort} onChange={sort => filter({ sort: sort as VodSort })} options={SORTS.map(([value, label]) => ({ value, label }))} />
            <span className="mv-match-count" role="status">{shown.length === pool.length ? `${pool.length} VODs` : `${shown.length} of ${pool.length}`}</span>
          </div>
          {!shown.length ? <div className="mv-empty"><p>No VODs match these filters</p><button type="button" className="mv-text-button" onClick={() => filter({ ...ANY_VOD, sort: filters.sort })}>Clear filters</button></div>
            : <div className={`mv-table${wide ? " is-wide" : ""}`}>
              <div className="mv-table-head" aria-hidden><span>Result</span><span>Player</span><span>Rank</span>{wide && <span>Opponent</span>}<span>Patch</span><span>Length</span><span>Moments</span><span>Title</span></div>
              <ul className="mv-vod-list" aria-label={wide ? `Every ${champion.name} VOD` : `${champion.name} matchup VODs`}>{shown.slice(0, limit).map(vod => { const name = vodPlayerName(vod.player); return <li key={vod.videoId}>
                <button type="button" className="mv-vod-row" data-vod-id={vod.videoId} ref={vod.videoId === lastOpened.current ? sourceRow : undefined} aria-label={`Watch ${name ?? vod.rankClaim}${wide ? ` vs ${enemyName(vod.enemy)}` : ""}: ${vod.title}`} onClick={() => { lastOpened.current = vod.videoId; setSelectedId(vod.videoId); }}>
                  <span className={`mv-cell-result${vod.result ? ` is-${vod.result === "W" ? "win" : "loss"}` : ""}`} title={vod.result === "W" ? "Win" : vod.result === "L" ? "Loss" : "Result unknown"}>{vod.result ?? "–"}</span>
                  <span className="mv-cell-player">{name ? <strong>{name}</strong> : <em>Unlisted</em>}{vod.team && <span className="mv-team-tag">{vod.team}</span>}</span>
                  <span className="mv-cell-rank"><RankCrest tier={rankTier(vod.rankClaim)} size={18} mode="desktop" /><span>{vod.rankClaim}</span></span>
                  {wide && <span className="mv-cell-enemy"><ChampionIcon id={vod.enemy ?? 0} name={enemyName(vod.enemy)} mode="desktop" /><span>{enemyName(vod.enemy)}</span><small>{LANES[vod.lane ?? ""]}</small></span>}
                  <span className="mv-cell-num">{vod.patch}</span><span className="mv-cell-num">{formatVodTime(vod.durationSec)}</span><span className="mv-cell-num">{vod.moments.length}</span>
                  <span className="mv-cell-title">{vod.title}</span>
                </button>
              </li>; })}</ul>
              {limit < shown.length && <NearEnd onNear={() => setLimit(current => current + ROW_STEP)} />}
            </div>}
        </>}
    </div>;
  }
  // Guide sections belong to the open matchup; a VOD from another one cannot link to them.
  const sameMatchup = !selected.enemy || (selected.enemy === enemy.id && selected.lane === lane);
  return <div className={`mv-player-view${active ? "" : " is-mini"}`} aria-label={active ? "VOD player" : "Mini player"} onKeyDown={event => {
    if (event.key !== "Escape" || event.defaultPrevented || event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
    event.preventDefault(); event.stopPropagation(); setSelectedId(null); setSeek(null);
  }}>
    <header className="mv-watch-header">
      <div className="mv-watch-toolbar"><button type="button" className="mv-back" ref={backButton} onClick={() => { setSelectedId(null); setSeek(null); }}><ArrowLeft size={15} aria-hidden />All VODs</button><span className="mv-eyebrow">Match room</span>
        {vodUrl(selected) && <GuideSourceButton url={vodUrl(selected)!} className="mv-youtube-link" label="Open on YouTube">Open on YouTube<ArrowUpRight size={14} /></GuideSourceButton>}
      </div>
      <div className="mv-description"><VodPlayer key={selected.videoId} vod={selected} /><div className="mv-game-info"><h2 className="mv-title">{selected.title}</h2><VodFacts vod={selected} /></div></div>
    </header>
    <div className="mv-mini-actions"><button type="button" onClick={onReopen} aria-label="Reopen VOD"><span className="mv-eyebrow">Now watching</span><strong>{vodPlayerName(selected.player) ?? `${selected.rankClaim} replay`}</strong><ArrowUpRight size={16} /></button><button type="button" aria-label="Close mini player" onClick={() => { setSelectedId(null); setSeek(null); }}><X size={16} /></button></div>
    <div className="mv-player-layout">
      <article className="mv-watch" aria-label="Selected VOD">
        <YoutubeVodPlayer key={selected.videoId} vod={selected} seek={seek} mini={!active} champions={champions} onMomentChange={setActiveMoment} />
      </article>
      <section className="mv-chapters" aria-label="Key moments"><header><div><span className="mv-eyebrow">Match story</span><h3>Key moments</h3></div><small>{String(selected.moments.length).padStart(2, "0")}</small></header>
          <ol className="mv-moments" ref={momentList}>{selected.moments.map((moment, i) => {
            const section = GUIDE_SECTIONS.find(([id]) => id === moment.section);
            return <li key={`${selected.videoId}-${i}`} data-kind={moment.kind} className={i === activeMoment ? "is-active" : undefined}>
            <button type="button" disabled={!playable} className="mv-moment" aria-current={i === activeMoment || undefined} aria-label={`Watch ${moment.label} at ${formatVodTime(moment.atSec)}`} onClick={() => setSeek({ moment, serial: (seek?.serial ?? 0) + 1 })}>
              <span className="mv-moment-time">{formatVodTime(moment.atSec)}</span>
              <span className="mv-moment-body"><span className="mv-moment-topline"><VodMomentArt moment={moment} champions={champions} index={i} /><span className="mv-moment-kind">{i === activeMoment ? 'Now playing' : moment.kind && moment.kind !== "other" ? moment.kind : "Key moment"}</span></span><span className="mv-moment-label">{moment.label}</span></span>

            </button>
            {section && sameMatchup && <button type="button" className="mv-section-link" onClick={() => onGuideSection(section[0])} aria-label={`Open ${section[1]} guide section`}>{section[1]}<ChevronRight size={12} aria-hidden /></button>}
          </li>; })}</ol>
      </section>
    </div>
  </div>;
}
