import { useEffect, useState } from "react";
import { Clock3 } from "lucide-react";
import { ChampionIcon } from "../runes/ChampionIcon";
import { ChampionSlot } from "./ChampionSlot";
import { roleSlug, secondsLeft, type Draft, type DraftMember } from "./champ-select-model";

const PHASES: Record<string, string> = { PLANNING: "Declare intent", BAN_PICK: "Picks & bans", FINALIZATION: "Finalizing", GAME_STARTING: "Game starting" };

/** Both teams' picks and hovers, your slot highlighted, bans and the phase timer. */
export function DraftBand({ draft, readAt, names, enemyIds, mainRoles }: { draft: Draft | null; readAt: number; names: Map<number, string>; enemyIds: number[]; mainRoles?: Map<number, string> }) {
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
  return <section className="fa-card cs-band" aria-label="Draft">
    <div className="cs-band-head">
      <div className="cs-band-side"><span className="cs-band-label is-ally">Your team</span><Bans ids={draft?.allyBans ?? []} names={names} label="Your team's bans" /></div>
      <span className="cs-band-phase"><span className="cs-live-dot" />{PHASES[draft?.timer?.phase ?? ""] ?? "Draft"}</span>
      <div className="cs-band-side is-enemy"><span className="cs-band-label is-enemy">Enemy team</span><Bans ids={draft?.enemyBans ?? []} names={names} label="Enemy team's bans" /></div>
    </div>
    <div className="cs-teams">
      {(["allies", "enemies"] as const).map(side => {
        const members = draft?.[side] ?? [];
        return <div className={`cs-team is-${side}`} key={side} aria-label={side === "allies" ? "Your team's picks" : "Enemy team's picks"}>
          {Array.from({ length: Math.max(5, members.length) }, (_, index) => {
            const member = members[index] ?? empty;
            return <Slot key={index} member={member} seat={(side === "enemies" && mainRoles?.get(member.championId)) || SEATS[index] || ""} name={names.get(member.championId)} laner={side === "enemies" && enemyIds.includes(member.championId) ? (enemyIds.length > 1 ? "Possible matchup" : "Your matchup") : ""} />;
          })}
        </div>;
      })}
      <div className="cs-draft-clock" aria-label={seconds !== null ? `${seconds} seconds remaining` : "Waiting for draft timer"} aria-live="off">
        <Clock3 size={15} aria-hidden />
        <strong>{seconds !== null ? seconds.toString().padStart(2, "0") : "—"}</strong>
        <span>{seconds !== null ? "seconds" : "Waiting"}</span>
      </div>
    </div>
  </section>;
}

function Bans({ ids, names, label }: { ids: number[]; names: Map<number, string>; label: string }) {
  return <div className="cs-bans" role="list" aria-label={label}>{Array.from({ length: 5 }, (_, index) => {
    const id = ids[index];
    return <span key={index} role="listitem" className={id ? "" : "is-empty"} aria-label={id ? `Banned: ${names.get(id) ?? "Champion"}` : "Empty ban slot"} title={id ? `Banned: ${names.get(id) ?? "Champion"}` : undefined}>{id > 0 && <ChampionIcon id={id} name={names.get(id)} mode="desktop" />}</span>;
  })}</div>;
}

// Practice tool, blind pick and ranked enemies give no positions: show the champion's usual role (enemies) or the seat's, dimmed.
const SEATS = ["top", "jungle", "mid", "adc", "support"];

function Slot({ member, seat, name, laner }: { member: DraftMember; seat: string; name?: string; laner: string }) {
  const hovering = member.championId > 0 && !member.locked;
  const known = roleSlug(member.position);
  const slug = known || seat;
  const state = member.local ? "Your pick" : laner || (hovering ? "Hovering" : "");
  const classes = [!known && "is-seat", member.local && "is-you", hovering && "is-hover", !!laner && "is-laner"].filter(Boolean).join(" ");
  return <ChampionSlot championId={member.championId} name={name} role={slug} state={state} locked={member.locked} className={classes} />;
}
