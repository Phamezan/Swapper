import { LockKeyhole } from "lucide-react";
import { RoleIcon } from "../runes/RoleIcon";
import { ChampionArt } from "./MatchupStrip";
import { NearViewport } from "./NearViewport";
import { DraftIcon } from "./ui/icons";
import "./champion-slot.css";

const ROLE_LABELS: Record<string, string> = { top: "Top", jungle: "Jungle", mid: "Mid", adc: "Bot", support: "Support" };

/** A champion or an empty role slot, shared by draft and matchup selection. */
export function ChampionSlot({ championId, name, role, state, locked, emptyLabel = "Not picked", className = "" }: {
  championId: number; name?: string; role: string; state?: string; locked?: boolean; emptyLabel?: string; className?: string;
}) {
  const label = ROLE_LABELS[role] ?? "";
  return <article className={`cs-slot${championId ? "" : " is-empty"} ${className}`} aria-label={[name || "Not picked", label, state, locked ? "locked in" : ""].filter(Boolean).join(" · ")}>
    <NearViewport className="cs-slot-art">{championId ? <ChampionArt id={championId} name={name ?? `Champion ${championId}`} kind="loading" /> : <span className="cs-slot-placeholder">{role ? <RoleIcon role={role} size={32} mode="desktop" /> : <DraftIcon size={28} strokeWidth={1.2} aria-hidden />}</span>}</NearViewport>
    <span className="cs-slot-role" title={label}>{role ? <RoleIcon role={role} size={16} mode="desktop" /> : <DraftIcon size={15} strokeWidth={1.5} aria-hidden />}</span>
    {locked && <LockKeyhole className="cs-slot-locked" size={13} aria-label="Locked in" />}
    <span className="cs-slot-copy">{state && <span className="cs-slot-state">{state}</span>}<strong>{championId ? name ?? "Champion" : emptyLabel}</strong><small>{label}</small></span>
  </article>;
}
