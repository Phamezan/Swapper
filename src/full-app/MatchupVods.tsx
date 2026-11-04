import { useEffect, useRef, useState } from "react";
import { ArrowLeft, ArrowUpRight, ChevronRight, Flag, X } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { VodMomentArt } from "./VodMomentArt";
import type { VodRequest } from "./MatchupVodEntry";
import type { ChampionOption } from "../runes/types";
import { ChampionIcon } from "../runes/ChampionIcon";
import { RankCrest } from "../runes/RankCrest";
import { GuideSourceButton } from "./GuideSourceButton";
import { loadAllVods, loadChampionVods } from "./loadMatchupVods";
import { GUIDE_SECTIONS, type GuideSectionId, type MatchupVod } from "./matchupGuide";
import { ANY_VOD, comparePatch, filterVods, formatVodTime, RANK_TIERS, rankTier, segmentOf, uploadAge, vodKey, vodMatches, vodPlayerName, vodRow, vodUrl, type RankTier, type VodFilters } from "./matchupVodModel";
import { NearEnd } from "./NearViewport";
import { Picker, ROLES } from "./Picker";
import { VodPlayer } from "./VodPlayer";
import { YoutubeVodPlayer, type VodSeek } from "./YoutubeVodPlayer";
import { usePlayerPhoto } from "./PlayerPhoto";
import { Skeleton } from "./ui/States";
import "./matchup-vods.css";

// Rows render in steps as the list scrolls, so a 1,000-VOD library opens as fast as a short one.
const ROW_STEP = 60;
const LANES: Record<string, string> = Object.fromEntries(ROLES);

function Segment<T>({ label, value, options, onChange }: { label: string; value: T; options: [T, string, number | null][]; onChange: (value: T) => void }) {
  return <div className="segmented mv-segment" role="group" aria-label={label}>{options.map(([option, text, count]) =>
    <button key={text} type="button" className={option === value ? "is-on" : undefined} aria-pressed={option === value} disabled={count === 0} onClick={() => onChange(option)}>{text}{count !== null && <small>{count}</small>}</button>)}</div>;
}

/** A named player's headshot when the CDN has a real one, otherwise the rank crest. */
function RowAvatar({ vod }: { vod: MatchupVod }) {
  const photo = usePlayerPhoto(vodPlayerName(vod.player));
  return photo ? <img className="mv-row-photo" src={photo} alt="" draggable={false} /> : <RankCrest tier={rankTier(vod.rankClaim)} size={26} mode="desktop" />;
}

/** Facts shared with the library; duration and moments live in the player controls. */
function VodFacts({ vod }: { vod: MatchupVod }) {
  const age = uploadAge(vod.uploaded, new Date());
  return <span className="mv-facts">{vodRow(vod).details.map(detail => <span key={detail} title={vod.patchEstimated && detail.startsWith("Patch ") ? "Patch estimated from the upload date" : undefined}>{detail}</span>)}
    {vod.result && <span className={`mv-result is-${vod.result === "W" ? "win" : "loss"}`}>{vod.result === "W" ? "Victory" : "Defeat"}</span>}
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
    void loadAllVods().then(vods => { if (live) setAll(vods); });
    return () => { live = false; };
  }, []);
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
    void loadChampionVods(id).then(vods => { if (live) setReverse({ key: id, vods: vods.map(vod => ({ ...vod, champion: id })) }); });
    return () => { live = false; };
  }, [champion?.id, filters.enemy]);
  const library = all ? all.filter(vod => !champion || vod.champion === champion.id) : null;
  const otherSide = champion && reverse?.key === filters.enemy ? reverse.vods.filter(vod => vod.enemy === champion.id) : null;
  const pool = (fromOtherSide ? otherSide : library) ?? [];
  const effectiveFilters = fromOtherSide ? { ...filters, enemy: null } : filters;
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
    const count = (next: Partial<VodFilters>) => pool.filter(vod => vodMatches(vod, { ...effectiveFilters, ...next })).length;
    const shown = filterVods(pool, effectiveFilters);
    const scopedCount = pool.filter(vod => vodMatches(vod, { ...ANY_VOD, enemy: effectiveFilters.enemy, lane: filters.lane })).length;
    const patches = [...new Set(pool.map(vod => vod.patch))].sort((a, b) => comparePatch(b, a));
    const championName = (id?: number) => champions.find(c => c.id === id)?.name ?? "Champion";
    const enemyName = (id?: number) => champions.find(c => c.id === id)?.name ?? "Opponent";
    const opponents = [...new Set(library?.flatMap(vod => vod.enemy ? [vod.enemy] : []) ?? [])].map(id => ({ id, name: enemyName(id),
      games: library!.filter(vod => vodMatches(vod, { ...filters, enemy: id })).length }))
      .sort((a, b) => b.games - a.games || a.name.localeCompare(b.name));
    const anyOpponentCount = library?.filter(vod => vodMatches(vod, { ...filters, enemy: null })).length ?? 0;
    const lanes = ROLES.filter(([id]) => library?.some(vod => vod.lane === id));
    const clear = () => { filter(ANY_VOD); setFromOtherSide(false); };
    return <div className="mv-library">
      <header className="mv-list-toolbar"><div><span className="mv-eyebrow">VOD library</span><h2>Browse VODs.</h2>
        <p>{library ? `${library.length} ${champion ? `${champion.name} VODs` : "VODs across all champions"}` : "Loading…"}</p></div>
        <div className="mv-scope"><Picker label="Champion" searchable value={String(champion?.id ?? 0)} onChange={id => {
          onChampionChange?.(champions.find(c => c.id === Number(id)) ?? null);
        }} options={[{ value: "0", label: "All champions", count: all?.length, disabled: all?.length === 0 }, ...champions.map(c => {
          const games = all?.filter(vod => vod.champion === c.id).length;
          return { value: String(c.id), label: c.name, count: games, disabled: games === 0, icon: <ChampionIcon id={c.id} name={c.name} mode="desktop" /> };
        })]} />{pool.some(vod => vod.fixture) && <span className="mv-preview">Preview data</span>}</div></header>
      {(filters.enemy || filters.lane) && <div className="mv-matchup-scope"><span>{champion?.name ?? "All champions"}{filters.enemy ? ` vs ${enemyName(filters.enemy)}` : ""}{filters.lane ? ` · ${LANES[filters.lane] ?? filters.lane}` : ""}</span>
        <button type="button" className="mv-text-button" onClick={clear}>{champion ? `Browse all ${champion.name} VODs` : "Clear filters"}</button></div>}
      {champion && filters.enemy && <Segment label="Perspective" value={fromOtherSide} onChange={value => { setFromOtherSide(value); filter({ named: false, streamer: false, tier: null, patch: null }); }}
        options={[[false, `As ${champion.name}`, library?.filter(vod => vodMatches(vod, { ...ANY_VOD, enemy: filters.enemy, lane: filters.lane })).length ?? null],
          [true, `From ${enemyName(filters.enemy)}’s side`, otherSide?.filter(vod => !filters.lane || vod.lane === filters.lane).length ?? null]]} />}
      {!library || (fromOtherSide && !otherSide) ? <Skeleton label="Loading VODs" rows={6} height={40} /> : <>
        {scopedCount >= 8 && <div className="mv-filters" role="group" aria-label="Filter VODs">
          <Segment label="Players" value={filters.named} onChange={named => filter({ named })} options={[[false, "Everyone", count({ named: false })], [true, "Named players", count({ named: true })]]} />
          {pool.some(vod => vod.format === "stream") && <Segment label="Streamer" value={filters.streamer} onChange={streamer => filter({ streamer })} options={[[false, "All VODs", count({ streamer: false })], [true, "Streamer", count({ streamer: true })]]} />}
          <Picker label="Rank" value={filters.tier ?? ""} onChange={tier => filter({ tier: (tier || null) as RankTier | null })} options={[{ value: "", label: "Any rank", count: count({ tier: null }), disabled: !count({ tier: null }) },
            ...RANK_TIERS.map(([value, label]) => { const games = count({ tier: value }); return { value, label, count: games, disabled: !games, icon: <RankCrest tier={value} size={22} mode="desktop" /> }; })]} />
          <Picker label="Patch" value={filters.patch ?? ""} onChange={patch => filter({ patch: patch || null })} options={[{ value: "", label: "Any patch", count: count({ patch: null }), disabled: !count({ patch: null }) },
            ...patches.map(patch => { const games = count({ patch }); return { value: patch, label: `Patch ${patch}`, count: games, disabled: !games }; })]} />
          <Picker label="Opponent" searchable value={String(filters.enemy ?? 0)} onChange={id => { filter({ enemy: Number(id) || null }); setFromOtherSide(false); }} options={[{ value: "0", label: "Any opponent", count: anyOpponentCount, disabled: !anyOpponentCount },
            ...opponents.map(({ id, name, games }) => ({ value: String(id), label: name, count: games, disabled: !games, icon: <ChampionIcon id={id} name={name} mode="desktop" /> }))]} />
          {lanes.length > 1 && <Picker label="Lane" value={filters.lane ?? ""} onChange={lane => filter({ lane: lane || null })} options={[{ value: "", label: "Any lane", count: count({ lane: null }), disabled: !count({ lane: null }) },
            ...lanes.map(([value, label]) => { const games = count({ lane: value }); return { value, label, count: games, disabled: !games }; })]} />}
          <span className="mv-match-count" role="status">{shown.length === pool.length ? `${pool.length} VODs` : `${shown.length} of ${pool.length}`}</span>
        </div>}
        {!shown.length ? <div className="mv-empty"><p>{!library.length && !fromOtherSide ? (champion ? `No ${champion.name} VODs yet. Choose another champion.` : "No VODs available yet. Try again later.") : fromOtherSide ? `No VODs from ${enemyName(filters.enemy ?? undefined)}’s side match these filters` : "No VODs match these filters"}</p>
          {(pool.length > 0 || filters.enemy || filters.lane) && <button type="button" className="mv-text-button" onClick={clear}>Clear filters</button>}</div>
          : <div className="mv-table is-wide"><ul className="mv-vod-list" aria-label={fromOtherSide ? `From ${enemyName(filters.enemy ?? undefined)}’s side` : champion ? `Every ${champion.name} VOD` : "All champion VODs"}>{shown.slice(0, limit).map(vod => {
            const row = vodRow(vod), age = uploadAge(vod.uploaded, new Date()), rowKey = `${vod.champion}-${vodKey(vod)}`;
            return <li key={rowKey}><button type="button" className="mv-vod-row" data-vod-id={vodKey(vod)} data-result={row.result ?? undefined} data-recent={rowKey === lastOpened.current || undefined}
              ref={rowKey === lastOpened.current ? sourceRow : undefined} title={vod.title} aria-label={`Watch ${row.primary}, ${championName(vod.champion)} vs ${vod.enemy ? enemyName(vod.enemy) : "any"}: ${vod.title}`}
              onClick={() => { lastOpened.current = rowKey; setSelection({ vod, fromOtherSide, watch: request }); setActiveMoment(-1); }}>
              <RowAvatar vod={vod} /><span className="mv-cell-who"><span className="mv-who-line"><strong className={row.named ? undefined : "is-unnamed"}>{row.primary}</strong>{vod.team && <span className="mv-team-tag">{vod.team}</span>}{vod.format === "stream" && <span className="mv-stream-tag">Streamer{vod.cam && <span className="mv-cam-dot" role="img" aria-label="Facecam" title="Facecam" />}</span>}</span>
                <small>{row.details.map((detail, i) => <span key={detail} title={vod.patchEstimated && detail.startsWith("Patch ") ? "Patch estimated from the upload date" : undefined}>{i > 0 && " · "}{detail}</span>)}{age && <span title={vod.uploaded}> · {age}</span>}</small></span>
              <span className="mv-cell-enemy"><ChampionIcon id={vod.champion ?? 0} name={championName(vod.champion)} mode="desktop" />{vod.enemy ? <ChampionIcon id={vod.enemy} name={enemyName(vod.enemy)} mode="desktop" /> : null}<span><span>{championName(vod.champion)} vs {vod.enemy ? enemyName(vod.enemy) : "any"}</span><small>{LANES[vod.lane ?? ""]}</small></span></span>
              <span className="mv-cell-moments">{row.moments && <><Flag size={12} aria-hidden />{row.moments}<span className="mv-hide-tight">{row.moments === 1 ? " moment" : " moments"}</span></>}</span>
              <span className="mv-cell-result">{row.result && <span className={`mv-result-badge is-${row.result}`}>{row.result === "win" ? "Win" : "Loss"}</span>}</span>
              <span className="mv-cell-length">{formatVodTime(segmentOf(vod).end - segmentOf(vod).start)}</span>
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
    <div className="mv-mini-actions"><button type="button" onClick={onReopen} aria-label="Reopen VOD"><span className="mv-eyebrow">Now watching</span><strong>{vodPlayerName(selected.player) ?? `${selected.rankClaim} replay`}</strong><ArrowUpRight size={16} /></button><button type="button" aria-label="Close mini player" onClick={close}><X size={16} /></button></div>
    <YoutubeVodPlayer key={vodKey(selected)} vod={selected} seek={seek} mini={!active} champions={champions} onMomentChange={setActiveMoment}
      header={<>
        <button type="button" className="mv-back" ref={backButton} onClick={close}><ArrowLeft size={15} aria-hidden />All VODs</button>
        <span className="mv-game-info" title={selected.title}><VodPlayer key={vodKey(selected)} vod={selected} />{selection?.fromOtherSide && <span className="mv-perspective-label">From {champions.find(c => c.id === selected.champion)?.name ?? "opponent"}’s side</span>}<VodFacts vod={selected} /></span>
        {vodUrl(selected) && <GuideSourceButton url={vodUrl(selected)!} className="mv-youtube-link" label="Open on YouTube">Open on YouTube<ArrowUpRight size={14} /></GuideSourceButton>}
      </>}>
      {selected.moments.length > 0 && <section className="mv-chapters" aria-label="Key moments"><header><h3>Key moments</h3><small>{selected.moments.length}</small></header>
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
