import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ChampionIcon } from "../runes/ChampionIcon";
import { ChampionOverview } from "../runes/ChampionOverview";
import { CardStrip } from "../runes/CounterStrips";
import { PresetBuild } from "../runes/PresetBuild";
import { CachedBadge } from "../runes/CachedBadge";
import { TIER_OPTIONS, fmtPct, tierClass } from "../runes/types";
import type { ChampionOption, ChampionOverviewView, ChampionCountersView, TierListView, MatchupView } from "../runes/types";
import "../runes/runes.css";

const lanes = [["top","Top"],["jungle","Jungle"],["mid","Mid"],["adc","Bot"],["support","Support"]];
const loadMatchup = (championId: number, enemyChampionId: number, position: string, tier: string) => invoke<MatchupView | null>("get_matchup", { championId, enemyChampionId, position, tier });

export default function Champions({ matchups }: { matchups: boolean }) {
  const [lane, setLane] = useState("mid");
  const [tier, setTier] = useState("emerald_plus");
  const [query, setQuery] = useState("");
  const [list, setList] = useState<TierListView | null>(null);
  const [champions, setChampions] = useState<ChampionOption[]>([]);
  const [selected, setSelected] = useState<ChampionOption | null>(null);
  const [enemy, setEnemy] = useState(0);
  const [detail, setDetail] = useState<MatchupView | null>(null);
  const [overview, setOverview] = useState<ChampionOverviewView | null>(null);
  const [counters, setCounters] = useState<ChampionCountersView | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
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
        invoke<ChampionCountersView>("get_champion_counters", { championId: selected.id, position: lane, tier }),
      ]).then(([overview,counters]) => { if (live) { setOverview(overview); setCounters(counters); } }).catch(reason => { if (live) setError(String(reason)); }).finally(() => { if (live) setLoading(false); });
    }
    return () => { live = false; };
  }, [selected, lane, tier]);
  useEffect(() => {
    let live = true; setDetail(null);
    if (selected && enemy && selected.id !== enemy) void loadMatchup(selected.id,enemy,lane,tier).then(value => { if (live) setDetail(value); }).catch(reason => { if (live) setError(String(reason)); });
    return () => { live = false; };
  }, [selected,enemy,lane,tier]);
  return <div className="champion-tools runes-panel">
    <div className="tool-filters"><label>Role<select value={lane} onChange={e => setLane(e.target.value)}>{lanes.map(([value,label]) => <option key={value} value={value}>{label}</option>)}</select></label><label>Rank<select value={tier} onChange={e => setTier(e.target.value)}>{TIER_OPTIONS.map(option => <option key={option.value} value={option.value}>{option.label}</option>)}</select></label>
      {matchups ? <><label>Your champion<select value={selected?.id ?? 0} onChange={e => setSelected(champions.find(c => c.id === Number(e.target.value)) ?? null)}><option value={0}>Choose champion</option>{champions.map(c => <option key={c.id} value={c.id}>{c.name}</option>)}</select></label><label>Opponent<select value={enemy} onChange={e => setEnemy(Number(e.target.value))}><option value={0}>Choose opponent</option>{champions.filter(c => c.id !== selected?.id).map(c => <option key={c.id} value={c.id}>{c.name}</option>)}</select></label></>
        : <label className="champion-search">Search<input placeholder="Find a champion…" value={query} onChange={e => setQuery(e.target.value)} /></label>}
    </div>
    {error && <p className="notice" role="alert">{error}</p>}
    {selected && <section className="selected-champion"><button className="back-to-list" onClick={() => setSelected(null)}>← {matchups ? "Clear champion" : "Back to tier list"}</button>
      {matchups && enemy > 0 && enemy !== selected.id && (detail ? <section className="matchup-banner" aria-label="Matchup">
        <div className="matchup-heading"><span className="matchup-portraits"><ChampionIcon id={selected.id} name={selected.name} mode="desktop" /><b>VS</b><ChampionIcon id={enemy} name={detail.enemyName} mode="desktop" /></span><div className="matchup-title"><strong>{selected.name} vs {detail.enemyName}</strong><small>{lanes.find(([value]) => value === lane)?.[1]} · {TIER_OPTIONS.find(option => option.value === tier)?.label}</small></div></div>
        {detail.stats ? <div className="matchup-stat-tiles"><div><small>YOUR WIN RATE</small><div><strong>{fmtPct(detail.stats.winPct)}</strong><span className={detail.stats.delta >= 0 ? "up" : "down"}> {detail.stats.delta >= 0 ? "+" : ""}{detail.stats.delta.toFixed(1)} vs average</span></div></div><div><small>{detail.stats.games.toLocaleString()} MATCHUP GAMES</small><div><strong>{fmtPct(detail.stats.avgWinPct)}</strong><span> champion average</span></div></div></div> : <p>No reliable data for this matchup yet.</p>}
        <p className="runes-note">{detail.fallback ? "General build · matchup sample unavailable or too small." : "Recommended build for this matchup"}</p><PresetBuild mode="desktop" build={detail.build} loading={false} matchupOnly />
      </section> : <p role="status">Loading matchup…</p>)}
      <ChampionOverview key={`${selected.id}-${lane}-${tier}`} mode="desktop" championId={selected.id} name={selected.name} view={overview} loading={loading} />
      {counters?.stale && <CachedBadge stale updatedAt={counters.updatedAt} />}
      {counters?.unavailable && <p className="notice">Counter data is currently unavailable.</p>}
      <CardStrip title="Toughest opponents" subtitle="Champions that perform well into this pick" tone="loss" rows={counters?.bestInto} loading={loading} mode="desktop" tier={tier} championId={selected.id} lane={lane} onLoadMatchup={loadMatchup} />
      <CardStrip title="Favourable matchups" subtitle="Champions this pick performs well against" tone="win" rows={counters?.beats} loading={loading} mode="desktop" tier={tier} championId={selected.id} lane={lane} onLoadMatchup={loadMatchup} />
    </section>}
    {!selected && (matchups ? <section className="empty"><h2>Know your lane</h2><p>Choose your champion and an opponent to compare the matchup, build and counters.</p></section> : <>
      {list?.stale && <CachedBadge stale updatedAt={list.updatedAt} />}
      {list?.unavailable ? <p className="notice">The tier list is currently unavailable. Try another role or rank.</p> : !list ? <p role="status">Loading tier list…</p> : <div className="tier-table-wrap"><table className="tier-table"><thead><tr>{["Rank","Champion","Lane","Tier","Win rate","Pick rate","Games"].map(label => <th key={label}>{label}</th>)}</tr></thead><tbody>{list.rows.map((row,index) => ({row,index})).filter(({row}) => row.name.toLowerCase().includes(query.trim().toLowerCase())).map(({row,index}) => <tr key={row.championId}><td>{index + 1}</td><td><button onClick={() => setSelected({ id:row.championId,name:row.name })}><ChampionIcon id={row.championId} name={row.name} mode="desktop" />{row.name}</button></td><td>{lanes.find(([value]) => value === lane)?.[1]}</td><td><span className={`tier-text ${tierClass(row.tier)}`}>{row.tier}</span></td><td>{fmtPct(row.winPct)}</td><td>{fmtPct(row.pickPct)}</td><td>{row.games.toLocaleString()}</td></tr>)}</tbody></table>{!list.rows.some(row => row.name.toLowerCase().includes(query.trim().toLowerCase())) && <p className="empty">No champions match this search.</p>}</div>}
    </>)}
  </div>;
}
