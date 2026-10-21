import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Search, Star } from "lucide-react";
import { ChampionIcon } from "../runes/ChampionIcon";
import { BuildPanel } from "./BuildPanel";
import { MatchupStrip, ChampionArt } from "./MatchupStrip";
import { RankPicker, ChampionPicker } from "./Picker";
import { NearViewport } from "./NearViewport";
import { PresetBuild } from "../runes/PresetBuild";
import { CachedBadge } from "../runes/CachedBadge";
import { RoleIcon } from "../runes/RoleIcon";
import { TIER_OPTIONS, fmtPct, tierClass } from "../runes/types";
import type { ChampionOption, ChampionOverviewView, ChampionCountersView, TierListView, TierRow, MatchupView } from "../runes/types";
import { useChampionArt } from "./art";
import { autoRows, mainRoles, ROLES, type AutoRow } from "./championsModel";
import { Notice, Skeleton } from "./ui/States";
import { fmtCount } from "./ui/format";
import "../runes/runes.css";

const lanes = [["top", "Top"], ["jungle", "Jungle"], ["mid", "Mid"], ["adc", "Bot"], ["support", "Support"]] as const;
const laneLabel = (lane: string) => lane === "auto" ? "Auto" : lanes.find(([value]) => value === lane)?.[1] ?? lane;
const tierLabel = (tier: string) => TIER_OPTIONS.find(option => option.value === tier)?.label ?? tier;
const loadMatchup = (championId: number, enemyChampionId: number, position: string, tier: string) => invoke<MatchupView | null>("get_matchup", { championId, enemyChampionId, position, tier });
type SortKey = "rank" | "winPct" | "pickPct" | "banPct" | "games";
const columns: [SortKey | null, string][] = [["rank", "Rank"], [null, "Champion"], [null, "Role"], [null, "Tier"], ["winPct", "Win rate"], ["pickPct", "Pick rate"], ["banPct", "Ban rate"], ["games", "Games"]];

export default function Champions({ matchups, focus }: { matchups: boolean; focus?: { champion: ChampionOption } | null }) {
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
  const [enemy, setEnemy] = useState(0);
  const [detail, setDetail] = useState<MatchupView | null>(null);
  const [overview, setOverview] = useState<ChampionOverviewView | null>(null);
  const [counters, setCounters] = useState<ChampionCountersView | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
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
  const open = async (champion: ChampionOption | null, filter = lane) => {
    if (!champion) { setSelected(null); return; }
    const next = filter !== "auto" ? filter : mainRoles(await loadRoleLists(tier)).get(champion.id) ?? "mid";
    setRole(next); setSelected(champion);
  };
  useEffect(() => { if (focus) void open(focus.champion); }, [focus]);
  useEffect(() => {
    let live = true;
    void invoke<ChampionOption[]>("get_champion_list").then(value => { if (live) setChampions(value); }).catch(reason => { if (live) setError(String(reason)); });
    return () => { live = false; };
  }, []);
  useEffect(() => {
    let live = true; setList(null); setError("");
    if (!matchups && lane === "auto") void loadRoleLists(tier).then(lists => { if (live) setList({ rows: autoRows(lists), unavailable: !Object.values(lists).some(rows => rows.length), stale: false, updatedAt: null }); });
    else if (!matchups) void invoke<TierListView>("get_tier_list", { position: lane, tier }).then(value => { if (live) setList(value); }).catch(reason => { if (live) setError(String(reason)); });
    return () => { live = false; };
  }, [lane, tier, matchups]);
  useEffect(() => {
    let live = true; setOverview(null); setCounters(null); setError("");
    if (selected) {
      setLoading(true);
      void Promise.all([
        invoke<ChampionOverviewView>("get_champion_overview", { championId: selected.id, position: role, tier }),
        invoke<ChampionCountersView>("full_app_matchups", { championId: selected.id, position: role, tier }),
      ]).then(([overview, counters]) => { if (live) { setOverview(overview); setCounters(counters); } }).catch(reason => { if (live) setError(String(reason)); }).finally(() => { if (live) setLoading(false); });
    }
    return () => { live = false; };
  }, [selected, role, tier]);
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

  // A champion page carries its own filters in its header, like a game client's champion screen.
  const inHeader = !!selected && !matchups;
  const pickLane = (value: string) => { setLane(value); if (selected) void open(selected, value); };
  const filters = <>
    <div className="fa-seg role-tabs" role="group" aria-label="Role">
      {!inHeader && <button aria-pressed={lane === "auto"} onClick={() => pickLane("auto")} title="Auto: each champion at its most played role"><Star size={15} aria-hidden />Auto</button>}
      {lanes.map(([value, label]) => <button key={value} aria-pressed={(inHeader ? role : lane) === value} onClick={() => inHeader ? setRole(value) : pickLane(value)}><RoleIcon role={value} size={16} mode="desktop" />{label}</button>)}
    </div>
    <RankPicker value={tier} onChange={setTier} />
  </>;
  return <div className="champion-tools runes-panel">
    <div className="tool-bar">
      {!inHeader && filters}
      {matchups ? <>
        <ChampionPicker label="Your champion" value={selected?.id ?? 0} champions={champions} onChange={id => { void open(champions.find(c => c.id === id) ?? null); if (enemy === id) setEnemy(0); }} />
        <span className="versus">vs</span>
        <ChampionPicker label="Opponent" value={enemy} champions={champions.filter(c => c.id !== selected?.id)} onChange={setEnemy} />
      </> : !selected && <label className="search-pill"><Search size={14} aria-hidden /><input placeholder="Search champion" aria-label="Search champion" value={query} onChange={e => setQuery(e.target.value)} /></label>}
    </div>
    {error && <Notice tone="error">{error}</Notice>}
    {selected && <section className="selected-champion">
      <ChampionHeader champion={selected} lane={role} tier={tier} onBack={() => setSelected(null)} backLabel={matchups ? "Clear champion" : "Back to tier list"} patch={overview?.stats?.patch}>{inHeader && filters}</ChampionHeader>
      {matchups && enemy > 0 && enemy !== selected.id && (detail ? <section className="matchup-banner" aria-label="Matchup">
        <div className="matchup-heading"><span className="matchup-portraits"><ChampionIcon id={selected.id} name={selected.name} mode="desktop" /><b>VS</b><ChampionIcon id={enemy} name={detail.enemyName} mode="desktop" /></span><div className="matchup-title"><strong>{selected.name} vs {detail.enemyName}</strong><small>{laneLabel(role)} · {tierLabel(tier)}</small></div></div>
        {detail.stats ? <div className="matchup-stat-tiles"><div><small>Your win rate</small><div><strong>{fmtPct(detail.stats.winPct)}</strong><span className={detail.stats.delta >= 0 ? "up" : "down"}> {detail.stats.delta >= 0 ? "+" : ""}{detail.stats.delta.toFixed(1)} vs average</span></div></div><div><small>{fmtCount(detail.stats.games)} matchup games</small><div><strong>{fmtPct(detail.stats.avgWinPct)}</strong><span> champion average</span></div></div></div> : <p>Not enough games for this matchup yet.</p>}
        <p className="runes-note">{detail.fallback ? "General build: the matchup sample is missing or too small." : "Recommended build for this matchup"}</p><PresetBuild mode="desktop" build={detail.build} loading={false} matchupOnly />
      </section> : <Skeleton label="Loading matchup" rows={2} height={72} />)}
      {loading && !overview ? <TableSkeleton /> : overview?.stats ? <>
        <div className={`fa-stat-strip ${tierClass(overview.stats.tier ?? "")}`}>
          <div><strong className="fa-tier-big">{overview.stats.tier ?? "–"}</strong><small>Tier</small></div>
          <div><strong>{overview.stats.rank ?? "–"}{overview.stats.rank ? <i> / {overview.stats.rankTotal}</i> : null}</strong><small>Lane rank</small></div>
          {([["Win rate", overview.stats.winPct], ["Pick rate", overview.stats.pickPct], ["Ban rate", overview.stats.banPct]] as const).map(([label, value]) => <div key={label}><strong><StatPct value={value} /></strong><small>{label}</small></div>)}
          <div><strong>{fmtCount(overview.stats.games)}</strong><small>Games</small></div>
        </div>
      </> : <Notice>No statistics available for this role and rank.</Notice>}
      <BuildPanel key={`build-${selected.id}-${role}-${tier}`} championId={selected.id} name={selected.name} lane={role} tier={tier} build={overview?.build ?? null} />
      {counters?.stale && <CachedBadge stale updatedAt={counters.updatedAt} />}
      <MatchupStrip key={`matchups-${selected.id}-${role}-${tier}`} championId={selected.id} name={selected.name} lane={role} tier={tier} counters={counters} onPick={setEnemy} />
      {!matchups && enemy > 0 && detail && <section className="fa-card fa-inline-matchup"><h3>{selected.name} vs {detail.enemyName}</h3><p>{detail.stats ? `${fmtPct(detail.stats.winPct)} win rate · ${fmtCount(detail.stats.games)} games · ${detail.stats.delta >= 0 ? "+" : ""}${detail.stats.delta.toFixed(1)}% versus champion average` : "Not enough games for this matchup."}</p><PresetBuild mode="desktop" build={detail.build} loading={false} matchupOnly /></section>}
    </section>}
    {!selected && (matchups ? <section className="empty"><h2>Pick your lane matchup</h2><p>Choose your champion and an opponent to see the matchup win rate, build and counters.</p></section> : <>
      {list && !query && <div className="fa-featured-picks">{list.rows.slice(0, 3).map(row => <button className="fa-featured-pick" key={row.championId} onClick={() => void open({ id: row.championId, name: row.name })}><NearViewport className="fa-featured-art"><ChampionArt id={row.championId} name={row.name} /></NearViewport><span className={`fa-tier ${tierClass(row.tier)}`}>{row.tier}</span><span><small>Leading {laneLabel((row as Partial<AutoRow>).role ?? lane).toLowerCase()} pick</small><strong>{row.name}</strong><b>{fmtPct(row.winPct)} <small>win rate</small></b></span></button>)}</div>}
      {list?.stale && <CachedBadge stale updatedAt={list.updatedAt} />}
      {list?.unavailable ? <Notice tone="warn">The tier list is unavailable right now. Try another role or rank.</Notice> : !list ? <TableSkeleton /> : <div className="tier-table-wrap">
        <table className="tier-table">
          <thead><tr>{columns.map(([key, label]) => <th key={label} aria-sort={key && key === sort ? (key === "rank" ? "ascending" : "descending") : undefined}>
            {key ? <button className={key === sort ? "is-sorted" : undefined} onClick={() => setSort(key)}>{label}</button> : label}</th>)}</tr></thead>
          <tbody>{rows.map(({ row, rank }) => <TierLine key={row.championId} row={row} rank={rank} lane={(row as Partial<AutoRow>).role ?? lane} average={averages.get((row as Partial<AutoRow>).role ?? lane) ?? 0} onOpen={() => void open({ id: row.championId, name: row.name })} />)}</tbody>
        </table>
        {!rows.length && <p className="panel-empty">No champion matches “{query}”.</p>}
      </div>}
    </>)}
  </div>;
}

function TierLine({ row, rank, lane, average, onOpen }: { row: TierRow; rank: number; lane: string; average: number; onOpen: () => void }) {
  const delta = row.winPct - average;
  return <tr onClick={onOpen}>
    <td className="col-rank">{rank}</td>
    <td><button className="tier-champ" onClick={event => { event.stopPropagation(); onOpen(); }}><NearViewport className="fa-tier-portrait"><ChampionIcon id={row.championId} name={row.name} mode="desktop" /></NearViewport><strong>{row.name}</strong></button></td>
    <td className="col-role"><RoleIcon role={lane} size={18} mode="desktop" /></td>
    <td><span className={`fa-tier ${tierClass(row.tier)}`}>{row.tier}</span></td>
    <td className="col-wr"><strong>{fmtPct(row.winPct)}</strong><small className={delta >= 0 ? "up" : "down"} title="Compared with this role’s average">{delta >= 0 ? "+" : "−"}{Math.abs(delta).toFixed(1)}</small></td>
    <td className="col-num">{fmtPct(row.pickPct)}</td>
    <td className="col-num ban">{fmtPct(row.banPct)}</td>
    <td className="col-num muted">{fmtCount(row.games)}</td>
  </tr>;
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
      <p className="fa-hero-sub">Build, runes and counters</p>
      {abilities.length > 0 && <div className="fa-hero-abilities">{abilities.map(ability => <span key={ability.key} title={ability.name}>{ability.icon && <img src={ability.icon} alt="" />}<kbd>{ability.key}</kbd></span>)}</div>}
    </div></div>
    {children && <div className="fa-hero-filters">{children}</div>}
  </header>;
}
