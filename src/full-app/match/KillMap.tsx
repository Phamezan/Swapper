import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ChampionIcon } from "../../runes/ChampionIcon";
import { mapPosition, personalKill, type MatchPlayer, type TimelineKill } from "../MatchDetailModel";
import { eventLabel, side, time } from "./GraphEvents";

let minimap: Promise<string> | undefined;
export type InspectEvent = (events: TimelineKill[], target: HTMLElement) => void;

export function KillMap({ kills, total, players, selected, onSelect, inspect, close, name }: {
  kills: TimelineKill[]; total: number; players: MatchPlayer[]; selected: number | null;
  onSelect: (id: number | null) => void; inspect: InspectEvent; close: () => void; name: (id: number) => string;
}) {
  const [src, setSrc] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    minimap ??= invoke<string>("objective_icon", {name: "map11"}).catch(error => { minimap = undefined; throw error; });
    void minimap.then(url => { if (live) setSrc(url); }).catch(() => {});
    return () => { live = false; };
  }, []);
  return <aside className="md-kill-map" aria-label="Champion kill map">
    <header><h3>Kill map</h3><span>{kills.length} of {total} positioned kills</span></header>
    <div className="md-map-surface">
      {src ? <img className="md-minimap" src={src} alt="Summoner's Rift minimap" onError={() => setSrc(null)} /> : <p>Map image unavailable</p>}
      {src && kills.map((event, i) => {
        const point = mapPosition(event.position)!;
        return <button key={i} className={`md-map-dot ${side(event.teamId)}${personalKill(event) ? " is-personal" : ""}`} style={{left: `${point.x}%`, top: `${point.y}%`}}
          aria-label={`${time(event.timestamp)} · ${eventLabel(event, name)}`} onPointerEnter={e => inspect([event], e.currentTarget)} onPointerLeave={close}
          onFocus={e => inspect([event], e.currentTarget)} onBlur={close} onClick={e => inspect([event], e.currentTarget)} />;
      })}
    </div>
    <div className="md-map-filter-heading"><span>Filter kills by player</span><button aria-pressed={selected == null} onClick={() => onSelect(null)}>All players</button></div>
    {[100, 200].map(teamId => <div className={`md-champion-filters ${side(teamId)}`} key={teamId} role="group" aria-label={`${teamId === 100 ? "Blue" : "Red"} team players`}>
      <span>{teamId === 100 ? "Blue" : "Red"}</span>
      {players.filter(player => player.teamId === teamId).map(player => <button key={player.participantId}
        className={player.me ? "is-me" : ""} aria-pressed={selected === player.participantId}
        aria-label={`Filter kills involving ${name(player.championId)}${player.me ? " (you)" : ""}`} title={`${name(player.championId)}${player.me ? " · You" : ""}`}
        onClick={() => onSelect(selected === player.participantId ? null : player.participantId)}>
        <ChampionIcon id={player.championId} name={name(player.championId)} mode="desktop" />
      </button>)}
    </div>)}
    <p className="md-map-note">Dots show the killing team. Ringed dots involve you. Selection also filters the event lanes.</p>
  </aside>;
}
