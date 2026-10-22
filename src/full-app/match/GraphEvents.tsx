import type { ResolvedTimelineEvent } from "../MatchDetailModel";
import { ChampionIcon } from "../../runes/ChampionIcon";
import { ObjectiveIcon } from "../ObjectiveIcon";

export const time = (ms: number) => `${Math.floor(ms / 60000)}:${String(Math.floor(ms / 1000) % 60).padStart(2, "0")}`;
export const side = (teamId: number) => teamId === 100 ? "is-blue" : "is-red";
export const eventLabel = (event: ResolvedTimelineEvent, name: (id: number) => string) => event.kind === "kill"
  ? `${name(event.killer.championId)} killed ${name(event.victim.championId)}${event.killer.me ? " · Your kill" : event.victim.me ? " · Your death" : ""}`
  : `${event.teamId === 100 ? "Blue" : "Red"} team took ${event.label.toLowerCase()}`;

export function EventContent({ event, name }: { event: ResolvedTimelineEvent; name: (id: number) => string }) {
  return <>
    {event.kind === "objective" ? <ObjectiveIcon label={event.objective} side={event.teamId} /> : <>
      <span className={`md-kill-portrait ${side(event.killer.teamId)}`}><ChampionIcon id={event.killer.championId} name={name(event.killer.championId)} mode="desktop" /></span>
      <span aria-hidden>→</span>
      <span className={`md-kill-portrait ${side(event.victim.teamId)}`}><ChampionIcon id={event.victim.championId} name={name(event.victim.championId)} mode="desktop" /></span>
    </>}
    <span className="md-event-description">{eventLabel(event, name)}</span>
  </>;
}
