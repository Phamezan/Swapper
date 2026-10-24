import { AbilityHover } from "./AbilityHover";
import { useEffect, useMemo, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { RefreshCw } from "lucide-react";
import { SearchIcon } from "./ui/icons";
import { ChampionIcon } from "../runes/ChampionIcon";
import { BuildPanel } from "./BuildPanel";
import { DuoTable } from "./DuoTable";
import { MatchupStrip, ChampionArt } from "./MatchupStrip";
import { ChampionSlot } from "./ChampionSlot";
import { RankPicker, ChampionPicker } from "./Picker";
import { NearEnd, NearViewport } from "./NearViewport";
import { PresetBuild } from "../runes/PresetBuild";
import { CachedBadge } from "../runes/CachedBadge";
import { RoleIcon } from "../runes/RoleIcon";
import { TIER_OPTIONS, fmtPct, isLowSample, tierClass } from "../runes/types";
import type { ChampionOption, ChampionOverviewView, ChampionCountersView, TierListView, TierRow, MatchupView } from "../runes/types";
import { useChampionArt } from "./art";
import { useAmbientChampion } from './AmbientArt';
import { autoRows, championPool, mainRoles, mostPlayed, ROLES, type AutoRow } from "./championsModel";
import type { DashboardView } from "./types";
import { Notice, Skeleton } from "./ui/States";
import { fmtCount } from "./ui/format";
import "../runes/runes.css";
import "./champions.css";

const lanes = [["top", "Top"], ["jungle", "Jungle"], ["mid", "Mid"], ["adc", "Bot"], ["support", "Support"]] as const;
const laneLabel = (lane: string) => lane === "auto" ? "Auto" : lanes.find(([value]) => value === lane)?.[1] ?? lane;
const tierLabel = (tier: string) => TIER_OPTIONS.find(option => option.value === tier)?.label ?? tier;
const loadMatchup = (championId: number, enemyChampionId: number, position: string, tier: string) => invoke<MatchupView | null>("get_matchup", { championId, enemyChampionId, position, tier });
// The tier list renders this many rows at first, then more as the page is scrolled.
const ROW_STEP = 40;
type SortKey = "rank" | "winPct" | "pickPct" | "banPct" | "games";
const columns: [SortKey | null, string][] = [["rank", "Rank"], [null, "Champion"], [null, "Role"], [null, "Tier"], ["winPct", "Win rate"], ["pickPct", "Pick rate"], ["banPct", "Ban rate"], ["games", "Games"]];

export default function Champions({ matchups, focus, onSelectionChange }: { matchups: boolean; focus?: { champion: ChampionOption | null } | null; onSelectionChange: (champion: ChampionOption | null) => void }) {
  // The list's role filter; "auto" lists every champion at its main role.
  const [lane, setLane] = useState("auto");
  // The opened champion's role: the filter's, or its main role under Auto.
  const [role, setRole] = useState("mid");
  const roleLists = useRef(new Map<string, Promise<Record<string, TierRow[]>>>());
  const [tier, setTier] = useState("emerald_plus");
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<SortKey>("rank");
  const [list, setList] = useState<TierListView | null>(null);
  const [champions, setChampions] = useState<ChampionOption[]>([]);
  const [selected, setSelected] = useState<ChampionOption | null>(null);
  useEffect(() => { onSelectionChange(selected); }, [selected, onSelectionChange]);
  useEffect(() => () => onSelectionChange(null), [onSelectionChange]);
  useAmbientChampion('champion', selected?.id);
  const [enemy, setEnemy] = useState(0);
  const hasEnemy = enemy > 0;
  const search = useRef<HTMLInputElement>(null);
  const backToOpponents = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (matchups) (hasEnemy ? backToOpponents.current : search.current)?.focus();
  }, [matchups, selected, hasEnemy]);
  const [detail, setDetail] = useState<MatchupView | null>(null);
  const [overview, setOverview] = useState<ChampionOverviewView | null>(null);
  const [counters, setCounters] = useState<ChampionCountersView | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [limit, setLimit] = useState(ROW_STEP);
  const [recent, setRecent] = useState<{ championId: number; role: string }[]>([]);
  const [matchupRoles, setMatchupRoles] = useState<Record<string, TierRow[]> | null>(null);
  const [showAll, setShowAll] = useState(false);
  const selectionRequest = useRef(0);
  // The five role tier lists share one cached u.gg file per rank, so loading all of them is cheap.
  const loadRoleLists = (forTier: string) => {
    let lists = roleLists.current.get(forTier);
    if (!lists) {
      lists = Promise.all(ROLES.map(r => invoke<TierListView>("get_tier_list", { position: r, tier: forTier }).then(view => [r, view.rows] as const).catch(() => [r, []] as const)))
        .then(entries => Object.fromEntries(entries));
      roleLists.current.set(forTier, lists);
    }
    return lists;
  };
  const open = async (champion: ChampionOption | null, filter = lane, forTier = tier) => {
    const request = ++selectionRequest.current;
    if (!champion) { setSelected(null); setEnemy(0); setQuery(""); return; }
    const next = filter !== "auto" && filter !== "duos" ? filter : mainRoles(await loadRoleLists(forTier)).get(champion.id) ?? "mid";
    if (request !== selectionRequest.current) return;
    setRole(next); setSelected(champion);
    if (matchups && !enemy) { setShowAll(false); setQuery(""); }
  };
  useEffect(() => () => { ++selectionRequest.current; }, []);
  useEffect(() => { if (focus) void open(focus.champion); }, [focus]);
  // Hovering a row starts its downloads; the backend caches them, so the click finds them ready.
  const prefetched = useRef(new Set<string>());
  const prefetch = (championId: number, role: string) => {
    const key = `${championId}|${role}|${tier}`;
    if (role === "auto" || prefetched.current.has(key)) return;
    prefetched.current.add(key);
    void invoke("get_champion_overview", { championId, position: role, tier }).catch(() => {});
    void invoke("full_app_matchups", { championId, position: role, tier }).catch(() => {});
  };
  useEffect(() => {
    let live = true;
    void invoke<ChampionOption[]>("get_champion_list").then(value => { if (live) setChampions(value); }).catch(reason => { if (live) setError(String(reason)); });
    return () => { live = false; };
  }, []);
  // Quick picks on the Matchups page come from the champions in the player's own recent games.
  useEffect(() => {
    if (!matchups) return;
    let live = true;
    // Recent games give roles; League Client's mastery fills in the rest of the pool (empty while the client is closed).
    void Promise.all([
      invoke<DashboardView>("dashboard_snapshot").then(view => mostPlayed(view.snapshot?.games ?? [], 6)).catch(() => []),
      invoke<{ championId: number; lastPlayed: number }[]>("full_app_champion_pool").catch(() => []),
    ]).then(([recent, mastery]) => { if (live) setRecent(championPool(recent, mastery)); });
    return () => { live = false; };
  }, [matchups]);
  useEffect(() => {
    let live = true; setList(null); setError("");
    if (matchups) {
      setMatchupRoles(null);
      void loadRoleLists(tier).then(lists => { if (live) setMatchupRoles(lists); });
    }
    else if (lane === "auto") void loadRoleLists(tier).then(lists => { if (live) setList({ rows: autoRows(lists), unavailable: !Object.values(lists).some(rows => rows.length), stale: false, updatedAt: null }); });
    else if (lane !== "duos") void invoke<TierListView>("get_tier_list", { position: lane, tier }).then(value => { if (live) setList(value); }).catch(reason => { if (live) setError(String(reason)); });
    return () => { live = false; };
  }, [lane, tier, matchups]);
  useEffect(() => {
    let live = true; setOverview(null); setCounters(null); setError("");
    if (selected && (!matchups || hasEnemy)) {
      setLoading(true);
      // The build shows as soon as it arrives; the matchup strip fills in on its own.
      void invoke<ChampionOverviewView>("get_champion_overview", { championId: selected.id, position: role, tier })
        .then(overview => { if (live) setOverview(overview); }).catch(reason => { if (live) setError(String(reason)); }).finally(() => { if (live) setLoading(false); });
      void invoke<ChampionCountersView>("full_app_matchups", { championId: selected.id, position: role, tier })
        .then(counters => { if (live) setCounters(counters); }).catch(reason => { if (live) setError(String(reason)); });
    }
    return () => { live = false; };
  }, [selected, role, tier, matchups, hasEnemy]);
  useEffect(() => {
    let live = true; setDetail(null);
    if (selected && enemy && selected.id !== enemy) void loadMatchup(selected.id, enemy, role, tier).then(value => { if (live) setDetail(value); }).catch(reason => { if (live) setError(String(reason)); });
    return () => { live = false; };
  }, [selected, enemy, role, tier]);

  // Each row's win rate is compared with its own role's average.
  const averages = useMemo(() => {
    const sums = new Map<string, [number, number]>();
    for (const row of list?.rows ?? []) {
      const key = (row as Partial<AutoRow>).role ?? lane;
      const [total, count] = sums.get(key) ?? [0, 0];
      sums.set(key, [total + row.winPct, count + 1]);
    }
    return new Map([...sums].map(([key, [total, count]]) => [key, total / count]));
  }, [list, lane]);
  const rows = useMemo(() => {
    const q = query.trim().toLowerCase();
    const ranked = (list?.rows ?? []).map((row, index) => ({ row, rank: index + 1 }));
    const filtered = ranked.filter(({ row }) => row.name.toLowerCase().includes(q));
    return sort === "rank" ? filtered : [...filtered].sort((a, b) => b.row[sort] - a.row[sort]);
  }, [list, query, sort]);
  useEffect(() => setLimit(ROW_STEP), [list, query, sort]);
  // Pick and ban bars are drawn against the highest rate in the list.
  const peak = useMemo(() => ({ pick: Math.max(1, ...(list?.rows ?? []).map(row => row.pickPct)), ban: Math.max(1, ...(list?.rows ?? []).map(row => row.banPct)) }), [list]);

  // A champion page carries its own filters in its header, like a game client's champion screen.
  const inHeader = !!selected && !matchups;
  const pickLane = (value: string) => { ++selectionRequest.current; setLane(value); if (selected) void open(selected, value); };
  const pickTier = (value: string) => { ++selectionRequest.current; setTier(value); if (matchups && selected) void open(selected, lane, value); };
  const picking = matchups && (!selected || !enemy);
  const suggested = new Set((matchupRoles?.[role] ?? []).map(row => row.championId));
  const cards = champions.filter(champion => champion.id !== selected?.id && champion.name.toLowerCase().includes(query.trim().toLowerCase())
    && (!selected || showAll || !suggested.size || suggested.has(champion.id))).sort((a, b) => a.name.localeCompare(b.name));
  const clearPick = () => { void open(null); search.current?.focus(); };
  const pickOpponent = () => { setEnemy(0); setQuery(""); setShowAll(false); };
  const filters = <>
    <div className="fa-seg role-tabs" role="group" aria-label="Role">
      {!inHeader && <button aria-pressed={lane === "auto"} onClick={() => pickLane("auto")} title="Auto: each champion at its most played role"><RoleIcon role="auto" size={16} mode="desktop" />Auto</button>}
      {lanes.map(([value, label]) => <button key={value} aria-pressed={(inHeader ? role : lane) === value} onClick={() => inHeader ? setRole(value) : pickLane(value)}><RoleIcon role={value} size={16} mode="desktop" />{label}</button>)}
      {!matchups && !inHeader && <button aria-pressed={lane === "duos"} onClick={() => pickLane("duos")}>Duos</button>}
    </div>
    <RankPicker value={tier} onChange={pickTier} />
  </>;
  return <div className="champion-tools runes-panel" onKeyDown={event => {
    if (!matchups || event.key !== "Escape" || event.defaultPrevented || (event.target as HTMLElement).closest('[data-radix-popper-content-wrapper]')) return;
    event.preventDefault(); event.stopPropagation();
    if (enemy) pickOpponent();
    else if (query) setQuery("");
    else clearPick();
  }}>
    <div className="tool-bar">
      {!inHeader && filters}
      {matchups && !picking ? <>
        <ChampionPicker label="Your champion" value={selected?.id ?? 0} champions={champions} onChange={id => { void open(champions.find(c => c.id === id) ?? null); if (enemy === id) setEnemy(0); }} />
        <span className="versus">vs</span>
        <ChampionPicker label="Opponent" value={enemy} champions={champions.filter(c => c.id !== selected?.id)} onChange={setEnemy} />
      </> : (!selected || picking) && <label className="search-pill"><SearchIcon size={14} aria-hidden /><input ref={search} placeholder={!matchups && lane === "duos" ? "Search duo" : selected ? "Search opponent" : "Search champion"} aria-label={!matchups && lane === "duos" ? "Search duo" : "Search champion"} value={query} onChange={e => setQuery(e.target.value)} /></label>}
    </div>
    {error && <Notice tone="error">{error}</Notice>}
    {selected && !picking && <section className="selected-champion">
      {matchups && <button ref={backToOpponents} className="back-to-list" onClick={pickOpponent}>← Back to opponents</button>}
      {/* On Matchups the versus banner is the hero once both champions are picked; the pickers above change or clear them. */}
      {!(matchups && enemy > 0 && enemy !== selected.id) && <ChampionHeader champion={selected} lane={role} tier={tier} onBack={() => setSelected(null)} backLabel={matchups ? "Clear champion" : "Back to tier list"} patch={overview?.stats?.patch}>{inHeader && filters}</ChampionHeader>}
      {matchups && enemy > 0 && enemy !== selected.id && (detail ? <section className="matchup-banner fa-versus" aria-label="Matchup">
        <VersusFrame />
        <VersusSide id={selected.id} name={selected.name} label="Your champion" />
        <div className="fa-versus-stat">
          {detail.stats ? <><small>{selected.name} win rate</small><strong className={detail.stats.winPct >= 50 ? "up" : "down"}>{fmtPct(detail.stats.winPct)}</strong>
            <span className="fa-versus-bar" role="img" aria-label={`${selected.name} ${fmtPct(detail.stats.winPct)}, ${detail.enemyName} ${fmtPct(100 - detail.stats.winPct)}`}><i style={{ width: `${detail.stats.winPct}%` }} /></span>
            <span>{fmtCount(detail.stats.games)} games{isLowSample(detail.stats.games) && <> · <em className="fa-low-sample" title="Under 100 games in this matchup: the win rate is a rough read, and the build below is the general one.">Low sample</em></>} · {laneLabel(role)} · <b className={detail.stats.delta >= 0 ? "up" : "down"}>{detail.stats.delta >= 0 ? "+" : "−"}{Math.abs(detail.stats.delta).toFixed(1)} vs avg</b></span></>
            : <><small>{laneLabel(role)} · {tierLabel(tier)}</small><span>Too few games.</span></>}
        </div>
        <VersusSide id={enemy} name={detail.enemyName} label="Opponent" end />
      </section> : <Skeleton label="Loading matchup" rows={2} height={72} />)}
      {loading && !overview ? <TableSkeleton /> : overview?.stats ? <>
        <div className={`fa-stat-strip ${tierClass(overview.stats.tier ?? "")}`}>
          <div><strong className="fa-tier-big">{overview.stats.tier ?? "–"}</strong><small>Tier</small></div>
          <div><strong>{overview.stats.rank ?? "–"}{overview.stats.rank ? <i> / {overview.stats.rankTotal}</i> : null}</strong><small>Lane rank</small></div>
          {([["Win rate", overview.stats.winPct], ["Pick rate", overview.stats.pickPct], ["Ban rate", overview.stats.banPct]] as const).map(([label, value]) => <div key={label}><strong><StatPct value={value} /></strong><small>{label}</small></div>)}
          <div><strong>{fmtCount(overview.stats.games)}</strong><small>Games</small></div>
        </div>
      </> : <Notice>No stats for this role and rank.</Notice>}
      <BuildPanel key={`build-${selected.id}-${role}-${tier}`} championId={selected.id} name={selected.name} lane={role} tier={tier} build={overview?.build ?? null} matchups={matchups && detail ? [detail] : []} preferMatchup={matchups} preview />
      {counters?.stale && <CachedBadge stale updatedAt={counters.updatedAt} />}
      <MatchupStrip key={`matchups-${selected.id}-${role}-${tier}`} championId={selected.id} name={selected.name} lane={role} tier={tier} counters={counters} onPick={matchups ? setEnemy : id => {
        // On a champion page a matchup card opens that champion's page, in the same lane; on Matchups it picks the opponent.
        const champion = champions.find(c => c.id === id);
        if (!champion) return;
        setEnemy(0);
        void open(champion, role);
        document.querySelector(".dashboard-main")?.scrollTo({ top: 0, behavior: "smooth" });
      }} />
      {!matchups && enemy > 0 && detail && <section className="fa-card fa-inline-matchup"><h3>{selected.name} vs {detail.enemyName}</h3><p>{detail.stats ? `${fmtPct(detail.stats.winPct)} win rate · ${fmtCount(detail.stats.games)} games · ${detail.stats.delta >= 0 ? "+" : ""}${detail.stats.delta.toFixed(1)}% versus champion average` : "Not enough games for this matchup."}</p><PresetBuild mode="desktop" build={detail.build} loading={false} matchupOnly /></section>}
    </section>}
    {picking && <section className="fa-matchup-empty">
      {selected ? <div className="fa-matchup-selection">
        <div className="fa-matchup-picks" role="group" aria-label="Matchup picks">
          <div className="fa-repick-slot">
            <ChampionSlot championId={selected.id} name={selected.name} role={role} state="Your champion" className="is-you" />
            <button type="button" className="fa-repick" aria-label="Change your champion" onClick={clearPick}><RefreshCw size={28} aria-hidden /><span>Change</span></button>
          </div>
          <span className="fa-matchup-vs" aria-hidden>VS</span>
          <ChampionSlot championId={0} role={role} state="Pick your opponent" emptyLabel={laneLabel(role)} />
        </div>
        <p className="fa-matchup-context" aria-live="polite"><RoleIcon role={role} size={16} mode="desktop" /> {laneLabel(role)} · {tierLabel(tier)}</p>
      </div> : <div className="fa-matchup-prompt" aria-live="polite">
        <h2>Pick your champion</h2><p>Choose your champion, then who you’re playing against.</p>
      </div>}
      {!selected && recent.length > 0 && <><h3>Your recent champions</h3><div className="fa-quick-picks">{recent.map(pick => {
        const champion = champions.find(c => c.id === pick.championId);
        return champion && <button key={pick.championId} className="fa-quick-pick" onClick={() => void open(champion)}><NearViewport className="fa-featured-art"><ChampionArt id={pick.championId} name={champion.name} /></NearViewport><span><strong>{champion.name}</strong><small>Recently played</small></span></button>;
      })}</div></>}
      <div className="fa-opponent-options">
        <h3>{selected ? showAll || !suggested.size ? "All opponents" : `${laneLabel(role)} opponents` : "All champions"}</h3>
        {selected && <button className="fa-show-all" aria-pressed={showAll} onClick={() => setShowAll(value => !value)}>Show all champions</button>}
      </div>
      {selected && !matchupRoles && <p role="status">Loading role suggestions…</p>}
      {selected && matchupRoles && !suggested.size && <p>Role suggestions unavailable. Showing all champions.</p>}
      <div className="fa-champion-grid" role="group" aria-label={selected ? "Choose opponent" : "Choose your champion"}>
        {cards.map(champion => <button key={champion.id} className="fa-champion-card" data-champion-id={champion.id}
            aria-label={`${champion.name}, ${selected ? "choose as opponent" : "choose as your champion"}`}
            onClick={() => selected ? setEnemy(champion.id) : void open(champion)}>
            <NearViewport className="fa-champion-card-art"><ChampionArt id={champion.id} name={champion.name} /></NearViewport>
            <span><strong>{champion.name}</strong></span>
          </button>)}
      </div>
      {!cards.length && <p>No {selected ? "opponent" : "champion"} matches{query ? ` “${query}”` : " this role"}.{selected && !showAll && " Try Show all champions."}</p>}
    </section>}
    {!selected && !matchups && (lane === "duos" ? <DuoTable tier={tier} query={query} /> : <>
      {list && !query && <div className="fa-featured-picks">{list.rows.slice(0, 3).map(row => <button className="fa-featured-pick" key={row.championId} onClick={() => void open({ id: row.championId, name: row.name })}><NearViewport className="fa-featured-art"><ChampionArt id={row.championId} name={row.name} /></NearViewport><span className={`fa-tier ${tierClass(row.tier)}`}>{row.tier}</span><span><small>Leading {laneLabel((row as Partial<AutoRow>).role ?? lane).toLowerCase()} pick</small><strong>{row.name}</strong><b>{fmtPct(row.winPct)} <small>win rate</small></b></span></button>)}</div>}
      {list?.stale && <CachedBadge stale updatedAt={list.updatedAt} />}
      {list?.unavailable ? <Notice tone="warn">Tier list unavailable. Try another role or rank.</Notice> : !list ? <TableSkeleton /> : <div className="tier-table-wrap">
        <table className="tier-table">
          <thead><tr>{columns.map(([key, label]) => <th key={label} aria-sort={key && key === sort ? (key === "rank" ? "ascending" : "descending") : undefined}>
            {key ? <button className={key === sort ? "is-sorted" : undefined} onClick={() => setSort(key)}>{label}</button> : label}</th>)}</tr></thead>
          <tbody>{rows.slice(0, limit).map(({ row, rank }) => <TierLine key={row.championId} row={row} rank={rank} lane={(row as Partial<AutoRow>).role ?? lane} average={averages.get((row as Partial<AutoRow>).role ?? lane) ?? 0} peak={peak} onOpen={() => void open({ id: row.championId, name: row.name })} onIntent={() => prefetch(row.championId, (row as Partial<AutoRow>).role ?? lane)} />)}</tbody>
        </table>
        {limit < rows.length && <NearEnd onNear={() => setLimit(current => current + ROW_STEP)} />}
        {!rows.length && <p className="panel-empty">No champion matches “{query}”.</p>}
      </div>}
    </>)}
  </div>;
}

function TierLine({ row, rank, lane, average, peak, onOpen, onIntent }: { row: TierRow; rank: number; lane: string; average: number; peak: { pick: number; ban: number }; onOpen: () => void; onIntent: () => void }) {
  const delta = row.winPct - average;
  // A short dwell, so sweeping the pointer across the table does not start a download per row.
  const dwell = useRef<number>(0);
  return <tr onClick={onOpen} onPointerEnter={() => { dwell.current = window.setTimeout(onIntent, 120); }} onPointerLeave={() => window.clearTimeout(dwell.current)} onFocus={onIntent}>
    <td className="col-rank">{rank}</td>
    <td><button className="tier-champ" onClick={event => { event.stopPropagation(); onOpen(); }}><NearViewport className="fa-tier-portrait"><ChampionIcon id={row.championId} name={row.name} mode="desktop" /></NearViewport><strong>{row.name}</strong></button></td>
    <td className="col-role"><RoleIcon role={lane} size={18} mode="desktop" /></td>
    <td><span className={`fa-tier ${tierClass(row.tier)}`}>{row.tier}</span></td>
    <td className="col-wr"><strong>{fmtPct(row.winPct)}</strong><small className={delta >= 0 ? "up" : "down"} title="Compared with this role’s average">{delta >= 0 ? "+" : "−"}{Math.abs(delta).toFixed(1)}</small></td>
    <td className="col-num"><RateBar value={row.pickPct} peak={peak.pick} /></td>
    <td className="col-num ban"><RateBar value={row.banPct} peak={peak.ban} /></td>
    <td className="col-num muted">{fmtCount(row.games)}</td>
  </tr>;
}

function RateBar({ value, peak }: { value: number; peak: number }) {
  return <><span>{fmtPct(value)}</span><i className="fa-rate-bar" style={{ "--rate": `${Math.min(100, (value / peak) * 100)}%` } as CSSProperties} aria-hidden /></>;
}

const VERSUS_PANELS = [["2,2 522,2 472,98 2,98", "blue", "#51a2e6"], ["528,2 998,2 998,98 478,98", "red", "#e83d32"]] as const;

/** The banner's two outlined panels, yours blue and theirs red, split by a slanted seam through the win rate:
 *  the fills sit behind the splash art, the glowing outlines in front of it. */
function VersusFrame() {
  return <><svg className="fa-versus-frame is-fill" viewBox="0 0 1000 100" preserveAspectRatio="none" aria-hidden>
    <defs>
      <linearGradient id="fa-versus-blue" gradientUnits="userSpaceOnUse" x1="0" x2="520"><stop offset="0" stopColor="#51a2e6" stopOpacity=".32" /><stop offset="1" stopColor="#51a2e6" stopOpacity=".03" /></linearGradient>
      <linearGradient id="fa-versus-red" gradientUnits="userSpaceOnUse" x1="480" x2="1000"><stop offset="0" stopColor="#e83d32" stopOpacity=".03" /><stop offset="1" stopColor="#e83d32" stopOpacity=".32" /></linearGradient>
      {/* Blur in viewBox units, pre-stretched for the wide banner so the glow stays about 3px all round. */}
      <filter id="fa-versus-glow" x="-5%" y="-30%" width="110%" height="160%"><feGaussianBlur stdDeviation="1.6 1.2" result="blur" /><feMerge><feMergeNode in="blur" /><feMergeNode in="SourceGraphic" /></feMerge></filter>
    </defs>
    {VERSUS_PANELS.map(([points, side]) => <polygon key={side} points={points} fill={`url(#fa-versus-${side})`} />)}
  </svg>
  <svg className="fa-versus-frame is-outline" viewBox="0 0 1000 100" preserveAspectRatio="none" aria-hidden>
    {VERSUS_PANELS.map(([points, side, color]) => <polygon key={side} points={points} fill="none" stroke={color} />)}
  </svg>
  {/* Each letter is clipped to its own panel, so it never crosses the seam or a border at any width. */}
  <span className="fa-versus-letter is-v" aria-hidden><i>V</i></span>
  <span className="fa-versus-letter is-s" aria-hidden><i>S</i></span></>;
}

function VersusSide({ id, name, label, end }: { id: number; name: string; label: string; end?: boolean }) {
  return <div className={`fa-versus-side${end ? " is-end" : ""}`}><NearViewport className="fa-versus-art"><ChampionArt id={id} name={name} /></NearViewport><span><small>{label}</small><strong>{name}</strong></span></div>;
}

function TableSkeleton() {
  return <div className="tier-table-wrap is-loading" role="status" aria-label="Loading tier list">{Array.from({ length: 8 }, (_, i) => <div key={i} className="skeleton-row" />)}</div>;
}

/** A percentage as a big number with a small, lighter percent sign. */
export function StatPct({ value }: { value: number | null | undefined }) {
  return value === null || value === undefined || Number.isNaN(value) ? <>–</> : <>{value.toFixed(1)}<i>%</i></>;
}

function ChampionHeader({ champion, lane, tier, patch, onBack, backLabel, children }: { champion: ChampionOption; lane: string; tier: string; patch?: string; onBack: () => void; backLabel: string; children?: ReactNode }) {
  const splash = useChampionArt(champion.id, "splash");
  const [abilities, setAbilities] = useState<{ key: string; name: string; icon: string | null }[]>([]);
  useEffect(() => {
    let live = true;
    void invoke<typeof abilities>("full_app_abilities", { championId: champion.id }).then(value => { if (live) setAbilities(value); }).catch(() => {});
    return () => { live = false; };
  }, [champion.id]);
  return <header className="champion-hero">
    {splash && <img className="hero-art" src={splash} alt="" draggable={false} />}
    <button className="back-to-list" onClick={onBack}>← {backLabel}</button>
    <div className="champion-hero-title"><ChampionIcon id={champion.id} name={champion.name} mode="desktop" /><div>
      <p className="fa-eyebrow"><RoleIcon role={lane} size={14} mode="desktop" /> {laneLabel(lane)} · {tierLabel(tier)}{patch ? ` · Patch ${patch}` : ""}</p>
      <h2>{champion.name}</h2>
      <p className="fa-hero-sub">Build, runes, counters</p>
      {abilities.length > 0 && <div className="fa-hero-abilities">{abilities.map(ability => <AbilityHover key={ability.key} championId={champion.id} abilityKey={ability.key} icon={ability.icon}><span>{ability.icon && <img src={ability.icon} alt="" />}<kbd>{ability.key}</kbd></span></AbilityHover>)}</div>}
    </div></div>
    {children && <div className="fa-hero-filters">{children}</div>}
  </header>;
}
