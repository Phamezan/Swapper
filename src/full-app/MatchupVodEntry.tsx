import { useEffect, useState } from "react";
import type { ChampionOption } from "../runes/types";
import { loadMatchupVods } from "./loadMatchupVods";
import { Button } from "../components/ui/button";
import { Play } from "lucide-react";
import "./matchup-vods.css";


export type VodRequest = { champion: ChampionOption | null; enemy?: ChampionOption; lane?: string };


export function MatchupVodEntry({ champion, enemy, lane, onWatch, compact = false }: VodRequest & { champion: ChampionOption; enemy: ChampionOption; lane: string; compact?: boolean; onWatch: (request: VodRequest) => void }) {
  const key = `${champion.id}-${enemy.id}-${lane}`;
  const [response, setResponse] = useState<{ key: string; count: number } | null>(null);
  useEffect(() => {
    let live = true;
    void loadMatchupVods(champion.id, enemy.id, lane).then(vods => { if (live) setResponse({ key, count: vods?.length ?? -1 }); });
    return () => { live = false; };
  }, [champion.id, enemy.id, lane, key]);
  const count = response?.key === key ? response.count : null;
  if (compact) return count === -1 ? <span>Couldn't load matchup VODs</span> : count ? <Button type="button" variant="default" onClick={() => onWatch({ champion, enemy, lane })}><Play aria-hidden /><span>Watch <b>{count.toLocaleString('en')}</b> VODs</span></Button> : null;
  return <aside className="mv-entry" aria-label="Matchup VODs"><span role="status">{count === -1 ? "Couldn't load matchup VODs" : count === null ? "Loading matchup VODs…" : count ? `${count.toLocaleString("en")} VODs for ${champion.name} vs ${enemy.name}` : "No VODs for this matchup"}</span>
    {count !== null && count >= 0 && <button type="button" className="mv-text-button" onClick={() => onWatch(count ? { champion, enemy, lane } : { champion })}>{count ? "Watch" : `Browse all ${champion.name} VODs`}</button>}
  </aside>;
}
