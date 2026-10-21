import { useEffect, useMemo, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Search } from "lucide-react";
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
import "../runes/runes.css";

const lanes = [["top", "Top"], ["jungle", "Jungle"], ["mid", "Mid"], ["adc", "Bot"], ["support", "Support"]] as const;
const laneLabel = (lane: string) => lanes.find(([value]) => value === lane)?.[1] ?? lane;
const tierLabel = (tier: string) => TIER_OPTIONS.find(option => option.value === tier)?.label ?? tier;
const loadMatchup = (championId: number, enemyChampionId: number, position: string, tier: string) => invoke<MatchupView | null>("get_matchup", { championId, enemyChampionId, position, tier });
type SortKey = "rank" | "winPct" | "pickPct" | "banPct" | "games";
const columns: [SortKey | null, string][] = [["rank", "Rank"], [null, "Champion"], [null, "Role"], [null, "Tier"], ["winPct", "Win rate"], ["pickPct", "Pick rate"], ["banPct", "Ban rate"], ["games", "Games"]];

export default function Champions({ matchups, focus }: { matchups: boolean; focus?: { champion: ChampionOption } | null }) {
  const [lane, setLane] = useState("mid");
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
  useEffect(() => { if (focus) setSelected(focus.champion); }, [focus]);
  useEffect(() => {
    let live = true;
    void invoke<ChampionOption[]>("get_champion_list").then(value => { if (live) setChampions(value); }).catch(reason => { if (live) setError(String(reason)); });
    return () => { live = false; };
  }, []);
  useEffect(() => {
    let live = true; setList(null); setError("");
    if (!matchups) void invoke<TierListView>("get_tier_list", { position: lane, tier }).then(value => { if (live) setList(value); }).catch(reason => { if (live) setError(String(reason)); });
    return () => { live = false; };
  }, [lane, tier, matchups]);
  useEffect(() => {
    let live = true; setOverview(null); setCounters(null); setError("");
    if (selected) {
      setLoading(true);
      void Promise.all([
        invoke<ChampionOverviewView>("get_champion_overview", { championId: selected.id, position: lane, tier }),
        invoke<ChampionCountersView>("full_app_matchups", { championId: selected.id, position: lane, tier }),
      ]).then(([overview, counters]) => { if (live) { setOverview(overview); setCounters(counters); } }).catch(reason => { if (live) setError(String(reason)); }).finally(() => { if (live) setLoading(false); });
    }
    return () => { live = false; };
  }, [selected, lane, tier]);
  useEffect(() => {
    let live = true; setDetail(null);
    if (selected && enemy && selected.id !== enemy) void loadMatchup(selected.id, enemy, lane, tier).then(value => { if (live) setDetail(value); }).catch(reason => { if (live) setError(String(reason)); });
    return () => { live = false; };
  }, [selected, enemy, lane, tier]);

  const average = useMemo(() => list?.rows.length ? list.rows.reduce((sum, row) => sum + row.winPct, 0) / list.rows.length : 0, [list]);
  const rows = useMemo(() => {
    const q = query.trim().toLowerCase();
    const ranked = (list?.rows ?? []).map((row, index) => ({ row, rank: index + 1 }));
    const filtered = ranked.filter(({ row }) => row.name.toLowerCase().includes(q));
    return sort === "rank" ? filtered : [...filtered].sort((a, b) => b.row[sort] - a.row[sort]);
  }, [list, query, sort]);

  const filters = <>
    <div className="fa-seg role-tabs" role="group" aria-label="Role">
      {lanes.map(([value, label]) => <button key={value} aria-pressed={lane === value} onClick={() => setLane(value)}><RoleIcon role={value} size={16} mode="desktop" />{label}</button>)}
    </div>
    <RankPicker value={tier} onChange={setTier} />
  </>;
  // A champion page carries its own filters in its header, like a game client's champion screen.
  const inHeader = !!selected && !matchups;
  return <div className="champion-tools runes-panel">
    <div className="tool-bar">
      {!inHeader && filters}
      {matchups ? <>
        <ChampionPicker label="Your champion" value={selected?.id ?? 0} champions={champions} onChange={id => { setSelected(champions.find(c => c.id === id) ?? null); if (enemy === id) setEnemy(0); }} />
        <span className="versus">vs</span>
        <ChampionPicker label="Opponent" value={enemy} champions={champions.filter(c => c.id !== selected?.id)} onChange={setEnemy} />
      </> : !selected && <label className="search-pill"><Search size={14} aria-hidden /><input placeholder="Search champion" aria-label="Search champion" value={query} onChange={e => setQuery(e.target.value)} /></label>}
    </div>
    {error && <p className="notice" role="alert">{error}</p>}
    {selected && <section className="selected-champion">
      <ChampionHeader champion={selected} lane={lane} tier={tier} onBack={() => setSelected(null)} backLabel={matchups ? "Clear champion" : "Back to tier list"} patch={overview?.stats?.patch}>{inHeader && filters}</ChampionHeader>
      {matchups && enemy > 0 && enemy !== selected.id && (detail ? <section className="matchup-banner" aria-label="Matchup">
        <div className="matchup-heading"><span className="matchup-portraits"><ChampionIcon id={selected.id} name={selected.name} mode="desktop" /><b>VS</b><ChampionIcon id={enemy} name={detail.enemyName} mode="desktop" /></span><div className="matchup-title"><strong>{selected.name} vs {detail.enemyName}</strong><small>{laneLabel(lane)} · {tierLabel(tier)}</small></div></div>
        {detail.stats ? <div className="matchup-stat-tiles"><div><small>Your win rate</small><div><strong>{fmtPct(detail.stats.winPct)}</strong><span className={detail.stats.delta >= 0 ? "up" : "down"}> {detail.stats.delta >= 0 ? "+" : ""}{detail.stats.delta.toFixed(1)} vs average</span></div></div><div><small>{detail.stats.games.toLocaleString()} matchup games</small><div><strong>{fmtPct(detail.stats.avgWinPct)}</strong><span> champion average</span></div></div></div> : <p>Not enough games for this matchup yet.</p>}
        <p className="runes-note">{detail.fallback ? "General build: the matchup sample is missing or too small." : "Recommended build for this matchup"}</p><PresetBuild mode="desktop" build={detail.build} loading={false} matchupOnly />
      </section> : <p role="status" className="loading-line">Loading matchup…</p>)}
      {loading && !overview ? <TableSkeleton /> : overview?.stats ? <>
        <div className={`fa-stat-strip ${tierClass(overview.stats.tier ?? "")}`}>
          <div><strong className="fa-tier-big">{overview.stats.tier ?? "–"}</strong><small>Tier</small></div>
          <div><strong>{overview.stats.rank ?? "–"}{overview.stats.rank ? <i> / {overview.stats.rankTotal}</i> : null}</strong><small>Lane rank</small></div>
          {([["Win rate", overview.stats.winPct], ["Pick rate", overview.stats.pickPct], ["Ban rate", overview.stats.banPct]] as const).map(([label, value]) => <div key={label}><strong><StatPct value={value} /></strong><small>{label}</small></div>)}
          <div><strong>{overview.stats.games.toLocaleString()}</strong><small>Games</small></div>
        </div>
      </> : <p className="notice">No statistics available for this role and rank.</p>}
      <BuildPanel key={`build-${selected.id}-${lane}-${tier}`} championId={selected.id} name={selected.name} lane={lane} tier={tier} build={overview?.build ?? null} />
      {counters?.stale && <CachedBadge stale updatedAt={counters.updatedAt} />}
      <MatchupStrip key={`matchups-${selected.id}-${lane}-${tier}`} championId={selected.id} name={selected.name} lane={lane} tier={tier} counters={counters} onPick={setEnemy} />
      {!matchups && enemy > 0 && detail && <section className="fa-card fa-inline-matchup"><h3>{selected.name} vs {detail.enemyName}</h3><p>{detail.stats ? `${fmtPct(detail.stats.winPct)} win rate · ${detail.stats.games.toLocaleString()} games · ${detail.stats.delta >= 0 ? "+" : ""}${detail.stats.delta.toFixed(1)}% versus champion average` : "Not enough games for this matchup."}</p><PresetBuild mode="desktop" build={detail.build} loading={false} matchupOnly /></section>}
    </section>}
    {!selected && (matchups ? <section className="empty"><h2>Pick your lane matchup</h2><p>Choose your champion and an opponent to see the matchup win rate, build and counters.</p></section> : <>
      {list && !query && <div className="fa-featured-picks">{list.rows.slice(0, 3).map(row => <button className="fa-featured-pick" key={row.championId} onClick={() => setSelected({ id: row.championId, name: row.name })}><NearViewport className="fa-featured-art"><ChampionArt id={row.championId} name={row.name} /></NearViewport><span className={`fa-tier ${tierClass(row.tier)}`}>{row.tier}</span><span><small>Leading {laneLabel(lane).toLowerCase()} pick</small><strong>{row.name}</strong><b>{fmtPct(row.winPct)} <small>win rate</small></b></span></button>)}</div>}
      {list?.stale && <CachedBadge stale updatedAt={list.updatedAt} />}
      {list?.unavailable ? <p className="notice">The tier list is unavailable right now. Try another role or rank.</p> : !list ? <TableSkeleton /> : <div className="tier-table-wrap">
        <table className="tier-table">
          <thead><tr>{columns.map(([key, label]) => <th key={label} aria-sort={key && key === sort ? (key === "rank" ? "ascending" : "descending") : undefined}>
            {key ? <button className={key === sort ? "is-sorted" : undefined} onClick={() => setSort(key)}>{label}</button> : label}</th>)}</tr></thead>
          <tbody>{rows.map(({ row, rank }) => <TierLine key={row.championId} row={row} rank={rank} lane={lane} average={average} onOpen={() => setSelected({ id: row.championId, name: row.name })} />)}</tbody>
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
    <td className="col-num muted">{row.games.toLocaleString()}</td>
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
