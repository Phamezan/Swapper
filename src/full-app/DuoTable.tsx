import { useEffect, useMemo, useState, type CSSProperties } from "react";
import { invoke } from "@tauri-apps/api/core";
import { CachedBadge } from "../runes/CachedBadge";
import { fmtPct, isLowSample, tierClass, type ChampionOption, type DuoListView } from "../runes/types";
import { ChampionArt } from "./MatchupStrip";
import { NearEnd, NearViewport } from "./NearViewport";
import { Notice, Skeleton } from "./ui/States";
import { fmtCount } from "./ui/format";
import { Tooltip } from "./ui/Tooltip";

const ROW_STEP = 40;
const columns = [["rank", "Rank"], [null, "Bot + Support"], [null, "Tier"], ["winPct", "Duo win rate"], ["games", "Games"]] as const;
type SortKey = "rank" | "winPct" | "games";

export function DuoTable({ tier, query }: { tier: string; query: string }) {
  const [view, setView] = useState<DuoListView | null>(null);
  const [error, setError] = useState("");
  const [sort, setSort] = useState<SortKey>("rank");
  const [champion, setChampion] = useState<ChampionOption | null>(null);
  const [limit, setLimit] = useState(ROW_STEP);
  useEffect(() => {
    let live = true; setView(null); setError("");
    void invoke<DuoListView>("get_duo_list", { tier }).then(value => { if (live) setView(value); }).catch(reason => { if (live) setError(String(reason)); });
    return () => { live = false; };
  }, [tier]);
  const rows = useMemo(() => {
    const q = query.trim().toLowerCase();
    // Keep the backend's scored order for Rank; raw win rate alone overvalues tiny samples.
    const ranked = (view?.rows ?? []).map((row, index) => ({ row, rank: index + 1 }));
    const filtered = ranked.filter(({ row }) => (!champion || row.adcId === champion.id || row.supportId === champion.id)
      && `${row.adcName} ${row.supportName}`.toLowerCase().includes(q));
    return sort === "rank" ? filtered : filtered.sort((a, b) => b.row[sort] - a.row[sort]);
  }, [view, query, sort, champion]);
  useEffect(() => setLimit(ROW_STEP), [view, query, sort, champion]);
  const all = view?.rows ?? [];
  const average = all.reduce((sum, row) => sum + row.winPct, 0) / (all.length || 1);
  const peakGames = Math.max(1, ...all.map(row => row.games));
  const toggleChampion = (id: number, name: string) => setChampion(current => current?.id === id ? null : { id, name });

  if (error) return <Notice tone="error">Could not load bot-lane duos. {error}</Notice>;
  if (!view) return <Skeleton label="Loading bot-lane duos" rows={8} height={88} />;
  return <>
    {view.stale && <CachedBadge stale updatedAt={view.updatedAt} />}
    {champion && <div className="fa-duo-filter"><button aria-label="Clear champion filter" onClick={() => setChampion(null)}>With {champion.name}<span aria-hidden>×</span></button></div>}
    {view.unavailable ? <Notice tone="warn">Duos unavailable. Try another rank.</Notice> : <div className="tier-table-wrap">
      <table className="tier-table fa-duo-table" aria-label="Bot-lane duos">
        <thead><tr>{columns.map(([key, label], i) => <th key={label} scope="col" aria-sort={key === sort ? (sort === "rank" ? "ascending" : "descending") : undefined} className={i === 0 ? "col-rank" : i === 1 ? "col-duo" : i === 2 ? "col-tier" : i === 3 ? "col-wr" : "col-num"}>
          {key ? <button className={key === sort ? "is-sorted" : undefined} onClick={() => setSort(key)}>{label}</button> : label}
        </th>)}</tr></thead>
        <tbody>{rows.slice(0, limit).map(({ row, rank }) => {
          const delta = row.winPct - average;
          return <tr key={`${row.adcId}-${row.supportId}`} className={tierClass(row.tier)}>
            <td className={`col-rank${rank <= 3 ? ` is-top-${rank}` : ""}`}><span className="fa-duo-rank-num">{rank}</span></td>
            <td className="col-duo"><div className="fa-duo-banner">
              <button className="fa-duo-half" aria-label={`Show duos with ${row.adcName}`} aria-pressed={champion?.id === row.adcId} onClick={() => toggleChampion(row.adcId, row.adcName)}><NearViewport className="fa-duo-art"><ChampionArt id={row.adcId} name={row.adcName} kind="splash" /></NearViewport></button>
              <button className="fa-duo-half is-support" aria-label={`Show duos with ${row.supportName}`} aria-pressed={champion?.id === row.supportId} onClick={() => toggleChampion(row.supportId, row.supportName)}><NearViewport className="fa-duo-art"><ChampionArt id={row.supportId} name={row.supportName} kind="splash" /></NearViewport></button>
              <span className="fa-duo-name"><strong>{row.adcName}</strong><small>Bot</small></span>
              <span className="fa-duo-name is-support"><strong>{row.supportName}</strong><small>Support</small></span>
            </div></td>
            <td className="col-tier"><span className={`fa-tier ${tierClass(row.tier)}`}>{row.tier}</span></td>
            <td className="col-wr"><strong>{fmtPct(row.winPct)}</strong><Tooltip label="Compared with the average duo in this list"><small className={delta >= 0 ? "up" : "down"}>{delta >= 0 ? "+" : "−"}{Math.abs(delta).toFixed(1)}</small></Tooltip></td>
            <td className="col-num"><div className="fa-duo-games"><b>{fmtCount(row.games)}</b><i className="fa-rate-bar" style={{ "--rate": `${(row.games / peakGames) * 100}%` } as CSSProperties} aria-hidden />{isLowSample(row.games) && <em className="fa-duo-low">Low sample</em>}</div></td>
          </tr>;
        })}</tbody>
      </table>
      {limit < rows.length && <NearEnd onNear={() => setLimit(current => current + ROW_STEP)} />}
      {!rows.length && <p className="panel-empty" role="status">{query.trim() ? `No duo matches “${query}”${champion ? ` with ${champion.name}` : ""}.` : champion ? `No duos with ${champion.name} for this rank yet.` : "No bot-lane duo data for this rank yet."}</p>}
    </div>}
  </>;
}
