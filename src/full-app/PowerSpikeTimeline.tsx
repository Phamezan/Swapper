import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { ChampionOption, ChampionOverviewView, ItemView, MatchupView } from "../runes/types";
import { GuideItem } from "./GuideItem";
import { GuideVisual } from "./GuideVisual";
import { Tooltip } from "./ui/Tooltip";
import { spikeMarkers } from "./matchupVisuals";
import "./matchup-visuals.css";

export function PowerSpikeTimeline({ champion, enemy, lane, tier, matchup }: {
  champion: ChampionOption; enemy: ChampionOption; lane: string; tier: string; matchup: MatchupView | null;
}) {
  const key = `${enemy.id}-${lane}-${tier}`;
  const [response, setResponse] = useState<{ key: string; items: ItemView[] } | null>(null);
  useEffect(() => {
    let live = true;
    void invoke<ChampionOverviewView>("get_champion_overview", { championId: enemy.id, position: lane, tier })
      .then(view => { if (live) setResponse({ key, items: view.build?.items.items ?? [] }); }).catch(() => {});
    return () => { live = false; };
  }, [enemy.id, lane, tier, key]);
  const own = matchup?.enemyChampionId === enemy.id ? matchup : null;
  const sides = [
    { side: "you", champion, markers: spikeMarkers(own?.build?.items ?? []), source: own?.fallback ? "Usual build" : "Matchup build" },
    { side: "enemy", champion: enemy, markers: spikeMarkers(response?.key === key ? response.items : []), source: "Usual build" },
  ].filter(side => side.markers.length);
  if (!sides.length) return null;
  return <GuideVisual title="Power spikes" preference="spikes" header={<small>Levels and purchase order</small>}>
    <div className="mg-spike-chart" aria-hidden="true">
      <div className="mg-spike-groups"><span /><span>Champion levels</span><span>Items in purchase order</span></div>
      <div className="mg-spike-axis"><span /><span>Level 6</span><span>Level 11</span><span>Level 16</span><span>1st item</span><span>2nd item</span><span>3rd item</span></div>
      {sides.map(side => <div className={`mg-spike-side is-${side.side}`} key={side.side}>
        <div className="mg-spike-identity"><strong>{side.champion.name}</strong><small>{side.source}</small></div>
        {side.markers.map(marker => marker.kind === "level"
          ? <div className="mg-spike-marker is-level" key={`level-${marker.level}`}><b>R{marker.rank}</b></div>
          : <Tooltip key={`item-${marker.order}`} label={marker.item.name}><div className={`mg-spike-marker is-item${marker.order === 1 ? " is-first-item" : ""}`}><GuideItem id={marker.item.id} size={32} label={marker.item.name} /><small>{marker.item.name}</small></div></Tooltip>)}
      </div>)}
    </div>
    <p className="mg-visual-caption">Typical R checkpoints · item columns show purchase order, not minute timings</p>
    <ul className="mg-visual-text">{sides.map(side => <li key={side.side}>{side.champion.name}, {side.source.toLowerCase()}: {side.markers.map(marker => marker.kind === "level" ? `R${marker.rank} checkpoint at level ${marker.level}` : `item ${marker.order}: ${marker.item.name}`).join("; ")}.</li>)}</ul>
  </GuideVisual>;
}
