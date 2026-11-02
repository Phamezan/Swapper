import { useEffect, useState } from "react";
import type { ChampionOption } from "../runes/types";
import { loadMatchupVods } from "./loadMatchupVods";
import "./matchup-vods.css";


export type VodRequest = { champion: ChampionOption | null; enemy?: ChampionOption; lane?: string };


export function MatchupVodEntry({ champion, enemy, lane, onWatch }: VodRequest & { champion: ChampionOption; enemy: ChampionOption; lane: string; onWatch: (request: VodRequest) => void }) {
  const key = `${champion.id}-${enemy.id}-${lane}`;
  const [response, setResponse] = useState<{ key: string; count: number } | null>(null);
  useEffect(() => {
    let live = true;
    void loadMatchupVods(champion.id, enemy.id, lane).then(vods => { if (live) setResponse({ key, count: vods.length }); });
    return () => { live = false; };
  }, [champion.id, enemy.id, lane, key]);
  const count = response?.key === key ? response.count : null;
  return <aside className="mv-entry" aria-label="Matchup VODs"><span role="status">{count === null ? "Loading matchup VODs…" : count ? `${count} VODs for ${champion.name} vs ${enemy.name}` : "No VODs for this matchup"}</span>
    {count !== null && <button type="button" className="mv-text-button" onClick={() => onWatch(count ? { champion, enemy, lane } : { champion })}>{count ? "Watch" : `Browse all ${champion.name} VODs`}</button>}
  </aside>;
}
