import { PageHeader } from "./ui/PageHeader";
import { useEffect, useMemo, useRef, useState } from "react";
import { ArrowLeft, ArrowUpRight, ChevronRight, Flag, Play, Scissors } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { VodMomentArt } from "./VodMomentArt";
import type { VodRequest } from "./MatchupVodEntry";
import type { ChampionOption } from "../runes/types";
import { ChampionIcon } from "../runes/ChampionIcon";
import { RankCrest } from "../runes/RankCrest";
import { TeamLogo } from "../runes/TeamLogo";
import { GuideSourceButton } from "./GuideSourceButton";
import { loadAllVods, loadChampionVods } from "./loadMatchupVods";
import { GUIDE_SECTIONS, type GuideSectionId, type MatchupVod } from "./matchupGuide";
import { ANY_VOD, facetCounts, playerInitials, vodLabel, vodThumbnail, type VodRow, comparePatch, filterVods, formatVodTime, RANK_TIERS, segmentOf, uploadAge, vodKey, vodMatches, vodPlayerName, vodRow, vodUrl, type RankTier, type VodFilters, teamName } from "./matchupVodModel";
import { NearEnd } from "./NearViewport";
import { Picker, ROLES } from "./Picker";
import { VodPlayer } from "./VodPlayer";
import { YoutubeVodPlayer, type VodSeek } from "./YoutubeVodPlayer";
import { usePlayerPhoto } from "./PlayerPhoto";
import { useChampionArt } from "./art";
import { Skeleton, Notice } from "./ui/States";
import "./matchup-vods.css";
// Rows render in steps as the list scrolls, so a 1,000-VOD library opens as fast as a short one.
const ROW_STEP = 60;
const LANES: Record<string, string> = Object.fromEntries(ROLES);
function Segment<T>({ label, value, options, onChange }: { label: string; value: T; options: [T, string, number | null][]; onChange: (value: T) => void }) {
  return <div className="segmented mv-segment" role="group" aria-label={label}>{options.map(([option, text, count]) =>
    <button key={text} type="button" className={option === value ? "is-on" : undefined} aria-pressed={option === value} disabled={count === 0} onClick={() => onChange(option)}>{text}{count !== null && <small>{count.toLocaleString("en")}</small>}</button>)}</div>;
}
function RowAvatar({ vod }: { vod: MatchupVod }) {
  const name = vodPlayerName(vod.player), photo = usePlayerPhoto(name);
  return photo ? <img className="mv-row-photo" src={photo} alt="" draggable={false} /> : <span className={`mv-row-initials${name ? "" : " is-unnamed"}`}>{name ? playerInitials(name) : "?"}</span>;
}
function RowThumb({ vod, row }: { vod: MatchupVod; row: VodRow }) {
  const [failed, setFailed] = useState(false);
  const src = vodThumbnail(vod), seg = segmentOf(vod);
  const splash = useChampionArt(!src || failed ? vod.enemy || vod.champion : null, "splash");
  return <span className="mv-thumb" aria-hidden>
    {src && !failed ? <img src={src} alt="" loading="lazy" decoding="async" draggable={false} width={320} height={180} onError={() => setFailed(true)} onLoad={event => { if (event.currentTarget.naturalWidth <= 120) setFailed(true); }} /> : <>
      {splash && <img className="mv-thumb-splash" src={splash} alt="" loading="lazy" decoding="async" draggable={false} width={320} height={180} />}
      <span className="mv-thumb-shade" />
    </>}
    <span className="mv-thumb-play"><Play size={12} fill="currentColor" /></span>
    {row.moments && <span className="mv-thumb-moments"><Flag size={10} />{row.moments}</span>}
    <span className="mv-cell-length" title={row.segment ? `Game from ${formatVodTime(row.segment.start)} to ${formatVodTime(row.segment.end)} of the stream` : undefined}>{row.segment && <Scissors size={10} aria-hidden />}{formatVodTime(seg.end - seg.start)}</span>
  </span>;
}
/** Patch and upload date follow the player identity in the header. */
function VodFacts({ vod }: { vod: MatchupVod }) {
  const age = uploadAge(vod.uploaded, new Date());
  return <span className="mv-facts"><span title={vod.patchEstimated ? "Patch estimated from the upload date" : undefined}>Patch {vodRow(vod).patch}</span>
    {age && <span title={vod.uploaded}>{age}</span>}
  </span>;
}

export function MatchupVods({ request, onChampionChange, onGuideSection, active = true, onReopen, onPlaybackChange }: {
  request: VodRequest;
  onChampionChange?: (champion: ChampionOption | null) => void; onGuideSection: (section: GuideSectionId, watch: VodRequest) => void;
  active?: boolean; onReopen?: () => void; onPlaybackChange?: (open: boolean) => void;
}) {
  const { champion, enemy, lane } = request;
  const [all, setAll] = useState<MatchupVod[] | null>(null);
  const [error, setError] = useState(false);
  const [retry, setRetry] = useState(0);
  const [reverse, setReverse] = useState<{ key: number; vods: MatchupVod[] } | null>(null);
  const [selection, setSelection] = useState<{ vod: MatchupVod; fromOtherSide: boolean; watch: VodRequest } | null>(null);
  const [seek, setSeek] = useState<VodSeek | null>(null);
  const [activeMoment, setActiveMoment] = useState(-1);
  const [champions, setChampions] = useState<ChampionOption[]>([]);
  const [fromOtherSide, setFromOtherSide] = useState(false);
  const [filters, setFilters] = useState<VodFilters>({ ...ANY_VOD, enemy: enemy?.id ?? null, lane: lane ?? null });
  const [limit, setLimit] = useState(ROW_STEP);
  const momentList = useRef<HTMLOListElement>(null);
  const lastOpened = useRef("");
  const sourceRow = useRef<HTMLButtonElement>(null);
  const backButton = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    let live = true;
    void invoke<ChampionOption[]>("get_champion_list").then(list => { if (live) setChampions(list); }).catch(() => {});
    return () => { live = false; };
  }, []);
  useEffect(() => {
    let live = true;
    void loadAllVods().then(vods => { if (live) { setAll(vods); setError(vods === null); } });
    return () => { live = false; };
  }, [retry]);
  useEffect(() => {
    setFilters({ ...ANY_VOD, enemy: enemy?.id ?? null, lane: lane ?? null });
    setFromOtherSide(false); setLimit(ROW_STEP);
    // Opening the browser resets filters while an already playing video keeps its own context.
    if (champion) { setSelection(null); setSeek(null); setActiveMoment(-1); }
  }, [request]);
  useEffect(() => {
    if (!champion || !filters.enemy) return;
    const id = filters.enemy;
    let live = true;
    void loadChampionVods(id).then(vods => { if (live) setReverse({ key: id, vods: (vods ?? []).map(vod => ({ ...vod, champion: id })) }); });
    return () => { live = false; };
  }, [champion?.id, filters.enemy]);
  const library = useMemo(() => all ? all.filter(vod => !champion || vod.champion === champion.id) : null, [all, champion?.id]);
  const otherSide = useMemo(() => champion && reverse?.key === filters.enemy ? reverse.vods.filter(vod => vod.enemy === champion.id) : null, [champion?.id, reverse, filters.enemy]);
  const pool = useMemo(() => (fromOtherSide ? otherSide : library) ?? [], [fromOtherSide, otherSide, library]);
  const effectiveFilters = useMemo(() => fromOtherSide ? { ...filters, enemy: null } : filters, [fromOtherSide, filters]);
  const counts = useMemo(() => facetCounts(pool, effectiveFilters), [pool, effectiveFilters]);
  const libraryCounts = useMemo(() => facetCounts(library ?? [], filters), [library, filters]);
  const shown = useMemo(() => filterVods(pool, effectiveFilters), [pool, effectiveFilters]);
  const championCounts = useMemo(() => { const result = new Map<number | undefined, number>(); for (const vod of all ?? []) result.set(vod.champion, (result.get(vod.champion) ?? 0) + 1); return result; }, [all]);
  const selected = selection?.vod;
  const playable = !!selected && !!vodUrl(selected);
  useEffect(() => {
    if (!active) return;
    if (selected) backButton.current?.focus({ preventScroll: true });
    else sourceRow.current?.focus();
  }, [selected, active]);
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
  if (!active && !selected) return null;
  if (!selected) {
    const filter = (next: Partial<VodFilters>) => { setFilters(previous => ({ ...previous, ...next })); setLimit(ROW_STEP); };
    const count = (next: Partial<VodFilters>) => { const key = Object.keys(next)[0] as keyof VodFilters; const value = next[key]; return counts[key].get(value === false ? null : value) ?? 0; };
    const scopedCount = pool.filter(vod => vodMatches(vod, { ...ANY_VOD, enemy: effectiveFilters.enemy, lane: filters.lane })).length;
    const patches = [...new Set(pool.map(vod => vod.patch))].sort((a, b) => comparePatch(b, a));
    const championName = (id?: number) => champions.find(c => c.id === id)?.name ?? "Champion";
    const enemyName = (id?: number) => champions.find(c => c.id === id)?.name ?? "Opponent";
    const opponents = [...new Set(library?.flatMap(vod => vod.enemy ? [vod.enemy] : []) ?? [])].map(id => ({ id, name: enemyName(id),
      games: libraryCounts.enemy.get(id) ?? 0 }))
      .sort((a, b) => b.games - a.games || a.name.localeCompare(b.name));
    const anyOpponentCount = libraryCounts.enemy.get(null) ?? 0;
    const lanes = ROLES.filter(([id]) => library?.some(vod => vod.lane === id));
    const clear = () => { filter(ANY_VOD); setFromOtherSide(false); };
    const wide = !(champion && filters.enemy && !fromOtherSide), now = new Date();
    return <div className="mv-library">
      <PageHeader title={<>VODs<span className="mv-head-count">{library ? <><b>{library.length.toLocaleString("en")}</b> {champion ? `${champion.name} VODs` : "across all champions"}</> : "Loading…"}</span></>} actions={pool.some(vod => vod.fixture) && <span className="mv-preview">Preview data</span>} />
      {(filters.enemy || filters.lane) && <div className="mv-matchup-scope"><span>{champion?.name ?? "All champions"}{filters.enemy ? ` vs ${enemyName(filters.enemy)}` : ""}{filters.lane ? ` · ${LANES[filters.lane] ?? filters.lane}` : ""}</span>
        <button type="button" className="mv-text-button" onClick={clear}>{champion ? `Browse all ${champion.name} VODs` : "Clear filters"}</button></div>}
      {error ? <Notice tone="error" action={<button onClick={() => { setError(false); setRetry(n => n + 1); }}>Retry</button>}>Couldn't load VODs.</Notice> : !library || (fromOtherSide && !otherSide) ? <Skeleton label="Loading VODs" rows={6} height={40} /> : <>
        {<div className="mv-filters" role="group" aria-label="Filter VODs">
<Picker label="Champion" searchable value={String(champion?.id ?? 0)} onChange={id => {
          onChampionChange?.(champions.find(c => c.id === Number(id)) ?? null);
        }} options={[{ value: "0", label: "All champions", count: all?.length, disabled: all?.length === 0 }, ...champions.map(c => {
          const games = championCounts.get(c.id) ?? 0;
          return { value: String(c.id), label: c.name, count: games, disabled: games === 0, icon: <ChampionIcon id={c.id} name={c.name} mode="desktop" /> };
        })]} />
          <Picker label="Opponent" searchable value={String(filters.enemy ?? 0)} onChange={id => { filter({ enemy: Number(id) || null }); setFromOtherSide(false); }} options={[{ value: "0", label: "Any opponent", count: anyOpponentCount, disabled: !anyOpponentCount },
            ...opponents.map(({ id, name, games }) => ({ value: String(id), label: name, count: games, disabled: !games, icon: <ChampionIcon id={id} name={name} mode="desktop" /> }))]} />
          {lanes.length > 1 && <Picker label="Lane" value={filters.lane ?? ""} onChange={lane => filter({ lane: lane || null })} options={[{ value: "", label: "Any lane", count: count({ lane: null }), disabled: !count({ lane: null }) },
            ...lanes.map(([value, label]) => { const games = count({ lane: value }); return { value, label, count: games, disabled: !games }; })]} />}
      {champion && filters.enemy && !!otherSide?.length && <Segment label="Perspective" value={fromOtherSide} onChange={value => { setFromOtherSide(value); filter({ named: false, streamer: false, tier: null, patch: null }); }}
        options={[[false, `As ${champion.name}`, library?.filter(vod => vodMatches(vod, { ...ANY_VOD, enemy: filters.enemy, lane: filters.lane })).length ?? null],
          [true, `From ${enemyName(filters.enemy)}’s side`, otherSide?.filter(vod => !filters.lane || vod.lane === filters.lane).length ?? null]]} />}
          {scopedCount >= 8 && <><span className="mv-filter-sep" aria-hidden />
          <Segment label="Players" value={filters.named} onChange={named => filter({ named })} options={[[false, "Everyone", count({ named: false })], [true, "Named players", count({ named: true })]]} />
          {pool.some(vod => vod.format === "stream") && <Segment label="Streamer" value={filters.streamer} onChange={streamer => filter({ streamer })} options={[[false, "All VODs", count({ streamer: false })], [true, "Streamer", count({ streamer: true })]]} />}
          <Picker label="Rank" value={filters.tier ?? ""} onChange={tier => filter({ tier: (tier || null) as RankTier | null })} options={[{ value: "", label: "Any rank", count: count({ tier: null }), disabled: !count({ tier: null }) },
            ...RANK_TIERS.map(([value, label]) => { const games = count({ tier: value }); return { value, label, count: games, disabled: !games, icon: <RankCrest tier={value} size={22} mode="desktop" /> }; })]} />
          <Picker label="Patch" value={filters.patch ?? ""} onChange={patch => filter({ patch: patch || null })} options={[{ value: "", label: "Any patch", count: count({ patch: null }), disabled: !count({ patch: null }) },
            ...patches.map(patch => { const games = count({ patch }); return { value: patch, label: `Patch ${patch}`, count: games, disabled: !games }; })]} />
          <Picker label="Region" value={filters.region ?? ""} onChange={region => filter({ region: region || null })} options={[{ value: "", label: "Any region", count: count({ region: null }) }, ...["KR", "EUW", "NA", "EUNE"].map(value => ({ value, label: value, count: count({ region: value }) }))]} />
          </>}
          {(filters.named || filters.streamer || filters.tier || filters.patch || filters.region) && <button className="mv-text-button" onClick={clear}>Clear filters</button>}
          <span className="mv-match-count" role="status">{shown.length.toLocaleString("en")} VODs</span>
        </div>}
        {!shown.length ? <div className="mv-empty"><p>{!library.length && !fromOtherSide ? (champion ? `No ${champion.name} VODs yet.` : "No VODs available yet. Try again later.") : fromOtherSide ? `No VODs from ${enemyName(filters.enemy ?? undefined)}’s side match these filters` : "No VODs match these filters"}</p>
          {!library.length && champion && <button className="mv-text-button" onClick={() => onChampionChange?.(null)}>Browse all champions</button>}
          {(pool.length > 0 || filters.enemy || filters.lane) && <button type="button" className="mv-text-button" onClick={clear}>Clear filters</button>}</div>
          : <div className={`mv-table${wide ? " is-wide" : ""}`}><div className="mv-vod-head" aria-hidden><span />{wide && <span>Matchup</span>}<span>Player</span><span>Rank</span><span>Patch</span><span className="is-end">Result</span></div><ul className="mv-vod-list" aria-label={fromOtherSide ? `From ${enemyName(filters.enemy ?? undefined)}’s side` : champion ? `Every ${champion.name} VOD` : "All champion VODs"}>{shown.slice(0, limit).map(vod => {
            const row = vodRow(vod), age = uploadAge(vod.uploaded, now), rowKey = `${vod.champion}-${vodKey(vod)}`;
            return <li key={rowKey}><button type="button" className="mv-vod-row" data-vod-id={vodKey(vod)} data-result={row.result ?? undefined} data-recent={rowKey === lastOpened.current || undefined}
              ref={rowKey === lastOpened.current ? sourceRow : undefined} title={vod.title} aria-label={vodLabel(vod, { champion: championName(vod.champion), enemy: vod.enemy ? enemyName(vod.enemy) : null, lane: LANES[vod.lane ?? ""] ?? null }, now)}
              onClick={() => { lastOpened.current = rowKey; setSelection({ vod, fromOtherSide, watch: request }); setActiveMoment(-1); }}>
              <RowThumb vod={vod} row={row} />
              {wide && <span className="mv-cell-matchup"><span className="mv-pair" aria-hidden><ChampionIcon id={vod.champion ?? 0} name="" mode="desktop" /><span className="mv-vs">vs</span><ChampionIcon id={vod.enemy ?? -1} name={vod.enemy ? "" : "Unknown opponent"} mode="desktop" /></span><span className="mv-lines"><strong>{championName(vod.champion)} <span className="mv-vs-word">vs</span> {vod.enemy ? enemyName(vod.enemy) : <span className="is-any">any</span>}</strong>{vod.lane && <small>{LANES[vod.lane]}</small>}</span></span>}
              <span className="mv-cell-who"><RowAvatar vod={vod} /><span className="mv-who-line"><strong className={row.named ? undefined : "is-unnamed"}>{row.named ? row.primary : "Ranked replay"}</strong>{vod.team && <span className="mv-team-tag" title={teamName(vod.team)}><TeamLogo team={teamName(vod.team)} size={16} mode="desktop" />{vod.team}</span>}{vod.format === "stream" && <span className="mv-stream-tag">Streamer{vod.cam && <span className="mv-cam-dot" role="img" aria-label="Facecam" title="Facecam" />}</span>}</span></span>
              <span className="mv-cell-rank" title={[row.rank, row.region].filter(Boolean).join(" · ")}>{row.crest ? <RankCrest tier={row.crest} size={26} mode="desktop" /> : <span />}<span className="mv-lines"><span className="mv-rank-tier">{row.rank}</span>{row.region && <small>{row.region}</small>}</span></span>
              <span className="mv-cell-patch"><span className={vod.patchEstimated ? "is-estimated" : undefined} title={vod.patchEstimated ? "Patch estimated from the upload date" : undefined}>{row.patch}</span>{age && <small title={vod.uploaded}>{age}</small>}</span>
              <span className="mv-cell-chips">{row.result && <span className={`mv-result-badge is-${row.result}`}>{row.result === "win" ? "Win" : "Loss"}</span>}</span>
            </button></li>;
          })}{limit < shown.length && <li className="mv-more"><NearEnd onNear={() => setLimit(current => current + ROW_STEP)} /></li>}</ul></div>}
      </>}
    </div>;
  }
  // Guide sections belong to the open matchup; a VOD from another one cannot link to them.
  const watch = selection!.watch;
  const sameMatchup = !selection?.fromOtherSide && !!watch.champion && !!watch.enemy && selected.champion === watch.champion.id && selected.enemy === watch.enemy.id && selected.lane === watch.lane;
  const close = () => { setSelection(null); setSeek(null); };
  return <div className={`mv-player-view${active ? "" : " is-mini"}`} aria-label={active ? "VOD player" : "Mini player"} onKeyDown={event => {
    if (event.key !== "Escape" || event.defaultPrevented || event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
    event.preventDefault(); event.stopPropagation(); close();
  }}>
    {active && <h1 className="sr-only">VOD player</h1>}

    <YoutubeVodPlayer key={vodKey(selected)} vod={selected} seek={seek} mini={!active} miniName={vodPlayerName(selected.player) ?? `${selected.rankClaim} replay`} onReopen={onReopen} onClose={close} onNextGame={(() => { const next = (all ?? []).filter(v => v.videoId === selected.videoId && (v.startSec ?? 0) > (selected.startSec ?? 0)).sort((a,b) => (a.startSec ?? 0) - (b.startSec ?? 0))[0]; return next ? () => { setSelection({ ...selection!, vod: next }); setSeek(null); } : undefined; })()} champions={champions} onMomentChange={setActiveMoment}
      header={<>
        <button type="button" className="mv-back" ref={backButton} onClick={close}><ArrowLeft size={15} aria-hidden />Back to VODs</button>
        <span className="mv-game-info" title={selected.title}>
          <span className="mv-header-game">
            <span className="mv-header-length" aria-label="Video length">{formatVodTime(segmentOf(selected).end - segmentOf(selected).start)}</span>
            <span className="mv-header-matchup" role="group" aria-label={`${champions.find(c => c.id === selected.champion)?.name ?? 'Champion'} vs ${selected.enemy ? champions.find(c => c.id === selected.enemy)?.name ?? 'Opponent' : 'unknown opponent'}`}>
              <ChampionIcon id={selected.champion ?? 0} name="" mode="desktop" /><strong>{champions.find(c => c.id === selected.champion)?.name ?? 'Champion'}</strong>
              <span>vs</span><ChampionIcon id={selected.enemy ?? -1} name="" mode="desktop" /><strong>{selected.enemy ? champions.find(c => c.id === selected.enemy)?.name ?? 'Opponent' : '?'}</strong>
            </span>
            <span>{LANES[selected.lane ?? '']}</span>
            <span className="mv-header-rank">{vodRow(selected).crest && <RankCrest tier={vodRow(selected).crest!} size={30} mode="desktop" />}{vodRow(selected).rank}</span>
          </span>
          <span className="mv-header-meta"><VodPlayer key={vodKey(selected)} vod={selected} />{selection?.fromOtherSide && <span className="mv-perspective-label">From {champions.find(c => c.id === selected.champion)?.name ?? "opponent"}’s side</span>}<VodFacts vod={selected} /></span>
        </span>
        {vodUrl(selected) && <GuideSourceButton url={vodUrl(selected)!} className="mv-youtube-link" label="Open on YouTube">Open on YouTube<ArrowUpRight size={14} /></GuideSourceButton>}
      </>}>
      {selected.moments.length > 0 && <section className="mv-chapters" aria-label="Key moments"><header><h3>Key moments</h3><small>{selected.moments.length} · [ ] jump · F fullscreen</small></header>
        <ol className="mv-moments" ref={momentList}>{selected.moments.map((moment, i) => {
          const section = GUIDE_SECTIONS.find(([id]) => id === moment.section);
          return <li key={`${vodKey(selected)}-${i}`} data-kind={moment.kind} className={i === activeMoment ? "is-active" : undefined}>
            <button type="button" disabled={!playable} className="mv-moment" aria-current={i === activeMoment || undefined} aria-label={`Watch ${moment.label} at ${formatVodTime(moment.atSec)}`} onClick={() => setSeek({ moment, serial: (seek?.serial ?? 0) + 1 })}>
              <span className="mv-moment-time">{formatVodTime(moment.atSec)}</span><VodMomentArt moment={moment} champions={champions} index={i} /><span className="mv-moment-label">{moment.label}</span>
            </button>
            {section && sameMatchup && <button type="button" className="mv-section-link" onClick={() => onGuideSection(section[0], watch)} aria-label={`Open ${section[1]} guide section`}>{section[1]}<ChevronRight size={12} aria-hidden /></button>}
          </li>; })}</ol>
      </section>}
    </YoutubeVodPlayer>
  </div>;
}
