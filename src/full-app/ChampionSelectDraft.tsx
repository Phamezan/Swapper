import { useEffect, useState } from "react";
import { Clock3, LockKeyhole } from "lucide-react";
import { DraftIcon } from "./ui/icons";
import { ChampionIcon } from "../runes/ChampionIcon";
import { RoleIcon } from "../runes/RoleIcon";
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
  const empty: DraftMember = { championId: 0, position: "", local: false, locked: false };
  return <section className={`fa-card cs-band${small ? " is-small" : ""}`} aria-label="Draft">
    <div className="cs-band-head">
      <span className="cs-band-label is-ally">Your team</span>
      <span className="cs-band-phase"><span className="cs-live-dot" />{PHASES[draft?.timer?.phase ?? ""] ?? "Draft"}</span>
      <span className="cs-band-label is-enemy">Enemy team</span>
    </div>
    <div className="cs-teams">
      {(["allies", "enemies"] as const).map(side => {
        const members = draft?.[side] ?? [];
        return <div className={`cs-team is-${side}`} key={side} aria-label={side === "allies" ? "Your team's picks" : "Enemy team's picks"}>
          {Array.from({ length: Math.max(5, members.length) }, (_, index) => {
            const member = members[index] ?? empty;
            return <Slot key={index} member={member} seat={SEATS[index] ?? ""} name={names.get(member.championId)} laner={side === "enemies" && member.championId > 0 && member.championId === enemyId} />;
          })}
        </div>;
      })}
      <div className="cs-draft-clock" aria-label={seconds !== null ? `${seconds} seconds remaining` : "Waiting for draft timer"} aria-live="off">
        <Clock3 size={15} aria-hidden />
        <strong>{seconds !== null ? seconds.toString().padStart(2, "0") : "—"}</strong>
        <span>{seconds !== null ? "seconds" : "Waiting"}</span>
      </div>
    </div>
    <div className="cs-band-foot"><span>Bans</span><div className="cs-bans" aria-label="Bans">{draft?.bans.length ? draft.bans.map((id, index) => <span key={`${id}-${index}`} title={`Banned: ${names.get(id) ?? "Champion"}`}><ChampionIcon id={id} name={names.get(id)} mode="desktop" /></span>) : <small>No bans yet</small>}</div><span className="cs-draft-hint">{small ? "Your pick is locked in" : "The draft updates live"}</span></div>
  </section>;
}

// Practice tool and blind pick give no positions: show each seat's usual role, dimmed.
const SEATS = ["top", "jungle", "mid", "adc", "support"];

function Slot({ member, seat, name, laner }: { member: DraftMember; seat: string; name?: string; laner: boolean }) {
  const hovering = member.championId > 0 && !member.locked;
  const known = roleSlug(member.position);
  const slug = known || seat;
  const role = ROLE_LABELS[slug] ?? "";
  const state = member.local ? "Your pick" : laner ? "Your matchup" : hovering ? "Hovering" : "";
  const classes = ["cs-slot", !known && "is-seat", member.local && "is-you", hovering && "is-hover", laner && "is-laner", !member.championId && "is-empty"].filter(Boolean).join(" ");
  return <article className={classes} aria-label={[name || "Not picked", role, state, member.locked ? "locked in" : ""].filter(Boolean).join(" · ")}>
    <NearViewport className="cs-slot-art">{member.championId ? <ChampionArt id={member.championId} name={name ?? `Champion ${member.championId}`} kind="loading" /> : <span className="cs-slot-placeholder">{slug ? <RoleIcon role={slug} size={32} mode="desktop" /> : <DraftIcon size={28} strokeWidth={1.2} aria-hidden />}</span>}</NearViewport>
    <span className="cs-slot-role" title={role}>{slug ? <RoleIcon role={slug} size={16} mode="desktop" /> : <DraftIcon size={15} strokeWidth={1.5} aria-hidden />}</span>
    {member.locked && <LockKeyhole className="cs-slot-locked" size={13} aria-label="Locked in" />}
    <span className="cs-slot-copy">{state && <span className="cs-slot-state">{state}</span>}<strong>{member.championId ? name ?? "Champion" : "Not picked"}</strong><small>{role}</small></span>
  </article>;
}
