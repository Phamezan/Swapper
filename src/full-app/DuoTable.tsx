import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ChampionIcon } from "../runes/ChampionIcon";
import { CachedBadge } from "../runes/CachedBadge";
import { fmtPct, tierClass, type ChampionOption, type DuoListView } from "../runes/types";
import { NearEnd } from "./NearViewport";
import { Notice, Skeleton } from "./ui/States";
import { fmtCount } from "./ui/format";

const ROW_STEP = 40;
const columns = [["rank", "Rank"], [null, "Bot + Support"], [null, "Tier"], ["winPct", "Duo win rate"], ["games", "Games together"]] as const;
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
  const toggleChampion = (id: number, name: string) => setChampion(current => current?.id === id ? null : { id, name });

  if (error) return <Notice tone="error">Could not load bot-lane duos. {error}</Notice>;
  if (!view) return <Skeleton label="Loading bot-lane duos" rows={8} height={52} />;
  return <>
    {view.stale && <CachedBadge stale updatedAt={view.updatedAt} />}
    {champion && <div className="fa-duo-filter"><button aria-label="Clear champion filter" onClick={() => setChampion(null)}>With {champion.name}<span aria-hidden>×</span></button></div>}
    {view.unavailable ? <Notice tone="warn">Duos unavailable. Try another rank.</Notice> : <div className="tier-table-wrap">
      <table className="tier-table fa-duo-table" aria-label="Bot-lane duos">
        <thead><tr>{columns.map(([key, label]) => <th key={label} scope="col" aria-sort={key === sort ? (sort === "rank" ? "ascending" : "descending") : undefined}>
          {key ? <button className={key === sort ? "is-sorted" : undefined} onClick={() => setSort(key)}>{label}</button> : label}
        </th>)}</tr></thead>
        <tbody>{rows.slice(0, limit).map(({ row, rank }) => <tr key={`${row.adcId}-${row.supportId}`}>
            <td className="col-rank">{rank}</td>
            <td><div className="fa-duo-identity">
              <span className="fa-duo-portraits">
                <button className="fa-duo-portrait" aria-label={`Show duos with ${row.adcName}`} aria-pressed={champion?.id === row.adcId} onClick={() => toggleChampion(row.adcId, row.adcName)}><ChampionIcon id={row.adcId} name={row.adcName} mode="desktop" /></button>
                <button className="fa-duo-portrait" aria-label={`Show duos with ${row.supportName}`} aria-pressed={champion?.id === row.supportId} onClick={() => toggleChampion(row.supportId, row.supportName)}><ChampionIcon id={row.supportId} name={row.supportName} mode="desktop" /></button>
              </span>
              <span className="fa-duo-names"><span><strong>{row.adcName}</strong><small>Bot</small></span><span><strong>{row.supportName}</strong><small>Support</small></span></span>
            </div></td>
            <td><span className={`fa-tier ${tierClass(row.tier)}`}>{row.tier}</span></td>
            <td className="col-wr"><strong>{fmtPct(row.winPct)}</strong></td>
            <td className="col-num muted">{fmtCount(row.games)}</td>
          </tr>)}</tbody>
      </table>
      {limit < rows.length && <NearEnd onNear={() => setLimit(current => current + ROW_STEP)} />}
      {!rows.length && <p className="panel-empty" role="status">{query.trim() ? `No duo matches “${query}”${champion ? ` with ${champion.name}` : ""}.` : champion ? `No duos with ${champion.name} for this rank yet.` : "No bot-lane duo data for this rank yet."}</p>}
    </div>}
  </>;
}
