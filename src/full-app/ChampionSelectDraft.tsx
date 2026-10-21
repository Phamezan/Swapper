import { useEffect, useState } from "react";
import { Clock3, LockKeyhole } from "lucide-react";
import { ChampionIcon } from "../runes/ChampionIcon";
import { ChampionArt } from "./MatchupStrip";
import { NearViewport } from "./NearViewport";
import { roleSlug, secondsLeft, type Draft, type DraftMember } from "./champ-select-model";

const ROLE_LABELS: Record<string, string> = { top: "Top", jungle: "Jungle", mid: "Mid", adc: "Bot", support: "Support" };
const PHASES: Record<string, string> = { PLANNING: "Declare intent", BAN_PICK: "Picks & bans", FINALIZATION: "Finalizing", GAME_STARTING: "Game starting" };

/** Both teams' picks and hovers, your slot highlighted, bans and the phase timer. */
export function DraftBand({ draft, readAt, names, enemyId, small }: { draft: Draft | null; readAt: number; names: Map<number, string>; enemyId: number; small: boolean }) {
  const [now, setNow] = useState(() => Date.now());
  const running = !!draft?.timer && draft.timer.remainingMs > 0;
  // One tick a second, only while a draft timer is counting down.
  useEffect(() => {
    if (!running) return;
    setNow(Date.now());
    const id = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(id);
  }, [running, readAt]);
  const seconds = secondsLeft(draft?.timer ?? null, readAt, now);
  const empty = Array.from({ length: 5 }, (): DraftMember => ({ championId: 0, position: "", local: false, locked: false }));
  return <section className={`fa-card cs-band${small ? " is-small" : ""}`} aria-label="Draft">
    <div className="cs-band-head">
      <span className="cs-band-label is-ally">Your team</span>
      {!!draft?.bans.length && <div className="cs-bans" aria-label="Bans">{draft.bans.map((id, index) => <span key={`${id}-${index}`} title={`Banned: ${names.get(id) ?? "Champion"}`}><ChampionIcon id={id} name={names.get(id)} mode="desktop" /></span>)}</div>}
      <span className="cs-timer" aria-live="off"><Clock3 size={14} aria-hidden />{PHASES[draft?.timer?.phase ?? ""] ?? "Champion select"}{seconds !== null && <b>{seconds}s</b>}</span>
      <span className="cs-band-label is-enemy">Enemy team</span>
    </div>
    <div className="cs-teams">
      {(["allies", "enemies"] as const).map(side => <div className={`cs-team is-${side}`} key={side}>{(draft?.[side]?.length ? draft[side] : empty).map((member, index) => <Slot key={index} member={member} name={names.get(member.championId)} laner={side === "enemies" && member.championId > 0 && member.championId === enemyId} />)}</div>)}
    </div>
  </section>;
}

function Slot({ member, name, laner }: { member: DraftMember; name?: string; laner: boolean }) {
  const hovering = member.championId > 0 && !member.locked;
  const role = ROLE_LABELS[roleSlug(member.position)] ?? "";
  const state = member.local ? "You" : laner ? "Lane" : hovering ? "Hovering" : role;
  const classes = ["cs-slot", member.local && "is-you", hovering && "is-hover", laner && "is-laner", !member.championId && "is-empty"].filter(Boolean).join(" ");
  return <div className={classes} title={[name, role, hovering ? "hovering" : member.locked ? "locked in" : ""].filter(Boolean).join(" · ") || undefined}>
    <NearViewport className="cs-slot-art">{member.championId ? <ChampionArt id={member.championId} name={name ?? `Champion ${member.championId}`} /> : <LockKeyhole size={18} aria-hidden />}</NearViewport>
    <span className="cs-slot-copy"><strong>{member.championId ? name ?? "Champion" : "Not picked"}</strong><small>{state}{member.local && role ? ` · ${role}` : ""}</small></span>
  </div>;
}
