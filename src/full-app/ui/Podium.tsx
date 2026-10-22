import { useEffect, useRef } from "react";
import { AddAccountIcon, SwitchIcon } from "./icons";
import type { Account } from "../../App";
import { BrandIcon } from "../../components/BrandIcon";
import { RankCrest } from "../../runes/RankCrest";
import type { AccountCard, Snapshot } from "../types";
import "./shell.css";

const apexTiers = new Set(["MASTER", "GRANDMASTER", "CHALLENGER"]);
const title = (text: string) => text.charAt(0) + text.slice(1).toLowerCase();

export type PodiumIdentity = {
  name: string; tag: string | null; icon: string | null; level: number | null; region: string | null;
  rank: Snapshot["rank"]; unranked: boolean;
};

/** Who is signed in: portrait with a connection dot, name, rank, and the account switcher. */
export function Podium({ who, connected, open, onToggle, crest }: {
  who: PodiumIdentity; connected: boolean; open: boolean; onToggle: () => void; crest: string | null;
}) {
  const { rank } = who;
  const lp = rank && !apexTiers.has(rank.tier) ? Math.max(0, Math.min(100, rank.lp)) : null;
  const label = `${who.name}${who.tag ? `#${who.tag}` : ""}${rank ? `, ${title(rank.tier)} ${rank.division}, ${rank.lp} LP` : ""}. ${connected ? "League Client connected." : "League Client not connected."} Switch account`;
  return <button type="button" className={`profile-podium${open ? " is-open" : ""}`} aria-haspopup="dialog" aria-expanded={open} aria-label={label} onClick={onToggle}>
    <span className="podium-portrait">
      {who.icon ? <img src={who.icon} alt="" /> : <BrandIcon size={34} />}
      {who.region && <span className="podium-region">{who.region}</span>}
      <i className={`podium-dot${connected ? "" : " is-off"}`} aria-hidden />
    </span>
    <span className="podium-base">
      <strong>{who.name}</strong>
      <span className="podium-tag">{who.tag ? `#${who.tag}` : "League Client"}{who.level != null && <> · Lv {who.level}</>}</span>
      <span className="podium-rank">
        {rank && crest && <RankCrest tier={crest} size={18} mode="desktop" />}
        {rank ? <>{title(rank.tier)} {rank.division}<span>· {rank.lp} LP</span></> : <span>{who.unranked ? "Unranked" : "Not signed in"}</span>}
      </span>
      {lp != null && <span className="podium-lp" role="img" aria-label={`${rank!.lp} of 100 LP to the next division`}><i style={{ width: `${lp}%` }} /></span>}
      <span className="podium-switch">Switch account <SwitchIcon size={16} /></span>
    </span>
  </button>;
}

const ago = (at: number) => {
  const days = Math.floor((Date.now() - at) / 864e5);
  return days < 1 ? "today" : days === 1 ? "1d ago" : `${days}d ago`;
};

/** The popover under the podium: saved accounts, Switch for the others. Close with Esc or a click outside. */
export function AccountPicker({ accounts, activeId, cards, disabled, onSwitch, onManage, onAdd, onClose }: {
  accounts: Account[]; activeId: string | null; cards: Record<string, AccountCard>; disabled: boolean;
  onSwitch: (id: string) => void; onManage: () => void; onAdd: () => void; onClose: () => void;
}) {
  const root = useRef<HTMLDivElement>(null);
  useEffect(() => {
    root.current?.querySelector<HTMLElement>("button:not(:disabled)")?.focus();
    const down = (event: PointerEvent) => {
      const target = event.target as Element;
      if (!root.current?.contains(target) && !target.closest(".profile-podium")) onClose();
    };
    const key = (event: KeyboardEvent) => { if (event.key === "Escape") onClose(); };
    document.addEventListener("pointerdown", down);
    document.addEventListener("keydown", key);
    return () => { document.removeEventListener("pointerdown", down); document.removeEventListener("keydown", key); };
  }, [onClose]);
  return <div className="account-picker" ref={root} role="dialog" aria-label="Switch account">
    <h4>Switch account</h4>
    {accounts.map(account => {
      const card = cards[account.id];
      const current = account.id === activeId;
      const detail = card?.rank ? `${title(card.rank.tier)} ${card.rank.division}, ${card.rank.lp} LP` : card ? "Unranked" : account.region ?? account.platform ?? "Saved session";
      return <div className={`picker-row${current ? " is-current" : ""}`} key={account.id}>
        {account.iconDataUrl ? <img src={account.iconDataUrl} alt="" loading="lazy" /> : <span className="picker-initial">{account.name[0]?.toUpperCase()}</span>}
        <span className="picker-copy"><b>{account.name}</b><small>{detail}</small>{card?.lastPlayed && !current && <small>Played {ago(card.lastPlayed)}</small>}</span>
        {current ? <span className="picker-tag">Signed in</span>
          : <button type="button" className="fa-secondary fa-switch is-compact" disabled={disabled} onClick={() => onSwitch(account.id)}>Switch</button>}
      </div>;
    })}
    {!accounts.length && <p className="picker-empty">No saved accounts yet.</p>}
    <div className="picker-foot">
      <button type="button" className="fa-text-button" onClick={onManage}>Manage accounts</button>
      <button type="button" className="fa-text-button" onClick={onAdd}><AddAccountIcon size={15} /> Add account</button>
    </div>
  </div>;
}

/** Dims the page while a switch runs. The switch flow reports no stages, so the bar is an honest indeterminate one. */
export function SwitchOverlay({ account, name }: { account: Account | undefined; name: string }) {
  return <div className="switch-overlay" role="status" aria-live="polite">
    <div className="switching-card ui-card is-raised">
      <div className="who">
        {account?.iconDataUrl ? <img src={account.iconDataUrl} alt="" /> : <span className="picker-initial">{name[0]?.toUpperCase()}</span>}
        <div><h2>Switching to {name}</h2><p>Signing in through Riot Client.</p></div>
      </div>
      <div className="switch-bar" aria-hidden><i /></div>
    </div>
  </div>;
}
