import { useState, type CSSProperties } from "react";
import { Popover } from "radix-ui";
import { CheckIcon, ForwardIcon, InfoIcon, RefreshIcon, SearchIcon, SettingsIcon, SwitchIcon } from "./icons";
import { Tooltip } from "./Tooltip";
import type { Account } from "../../App";
import { BrandIcon } from "../../components/BrandIcon";
import { RankCrest } from "../../runes/RankCrest";
import type { AccountCard, Snapshot } from "../types";
import "./shell.css";

const addHint = "Sign in through Riot Client to save another account.";
const apexTiers = new Set(["MASTER", "GRANDMASTER", "CHALLENGER"]);
const title = (text: string) => text.charAt(0) + text.slice(1).toLowerCase();
const crestOf = (rank: Snapshot["rank"]) => rank ? rank.tier.toLowerCase().replace(/\s+/g, "") : null;
const rankLabel = (rank: NonNullable<Snapshot["rank"]>) => `${title(rank.tier)}${apexTiers.has(rank.tier) ? "" : ` ${rank.division}`}`;

export type PodiumIdentity = {
  name: string; tag: string | null; icon: string | null; level: number | null; region: string | null;
  rank: Snapshot["rank"]; unranked: boolean;
};

/** Sidebar identity, with a separate account selector anchored at its foot. */
export function Podium({ who, connected, open, crest, disabled, pending, trouble, onRestart, onOpenProfile }: {
  who: PodiumIdentity; connected: boolean; open: boolean; crest: string | null; disabled: boolean;
  /** A switch to this account is under way. */
  pending?: boolean;
  /** League Client is running but not answering; offers a restart. */
  trouble?: boolean; onRestart?: () => void; onOpenProfile?: () => void;
}) {
  const { rank } = who;
  const lp = rank && !apexTiers.has(rank.tier) ? Math.max(0, Math.min(100, rank.lp)) : null;
  const label = `${who.name}${who.tag ? `#${who.tag}` : ""}${rank ? `, ${rankLabel(rank)}, ${rank.lp} LP` : ""}. ${connected ? "League Client connected." : "League Client not connected."}`;
  return <section className={`profile-podium${pending ? " is-pending" : ""}`} aria-label={label} aria-busy={pending || undefined}>
    <span className="podium-hero" aria-hidden />
    <Tooltip label="Open your profile"><button type="button" className="podium-band" onClick={onOpenProfile} disabled={!onOpenProfile}>
      <span className="podium-portrait">
        {who.icon ? <img src={who.icon} alt="" /> : <BrandIcon size={40} />}
        {who.level != null && <span className="podium-level">Lv {who.level}</span>}
        <i className={`podium-dot${connected ? "" : " is-off"}`} aria-hidden />
      </span>
      <span className="podium-copy">
        <strong>{who.name}{who.tag && <span className="podium-tagpart">#{who.tag}</span>}</strong>
      </span>
    </button></Tooltip>
    <span className="podium-standing">
      <span className="podium-rank">
        {rank && crest && <RankCrest tier={crest} size={40} mode="desktop" />}
        <span className="podium-rank-copy">
          <b>{pending ? "Signing in…" : rank ? rankLabel(rank) : who.unranked ? "Unranked" : "Not signed in"}</b>
          {rank && !pending && <span className="podium-lptext"><b>{rank.lp} LP</b></span>}
        </span>
      </span>
      {lp != null && <span className="podium-lp" role="img" aria-label={`${rank!.lp} of 100 LP to the next division`}><i style={{ width: `${lp}%` }} /></span>}
    </span>
    {trouble && <div className="podium-status" role="status"><span>League Client isn't responding</span>
      <button type="button" disabled={disabled} onClick={onRestart}><RefreshIcon size={14} aria-hidden />Restart client</button></div>}
    <Popover.Trigger asChild>
      <button type="button" className="podium-switch" disabled={disabled}>
        <SwitchIcon size={17} aria-hidden /><span>Switch account</span>
        <span className={`podium-chev${open ? " is-open" : ""}`} aria-hidden><ForwardIcon size={16} /></span>
      </button>
    </Popover.Trigger>
  </section>;
}

/** Account rows keep identity and rank together; Popover owns placement and dismissal. */
export function AccountPicker({ accounts, activeId, activeRank, cards, disabled, closed, onSwitch, onRestart, onManage }: {
  accounts: Account[]; activeId: string | null; cards: Record<string, AccountCard>; disabled: boolean; closed?: boolean;
  activeRank: Snapshot["rank"]; onSwitch: (id: string) => void; onRestart: () => void; onManage: () => void;
}) {
  const [query, setQuery] = useState("");
  const needle = query.trim().toLowerCase();
  const shown = accounts.filter(account => !needle || [account.name, account.nickname, account.riotId].some(value => value?.toLowerCase().includes(needle)));
  return <Popover.Portal><Popover.Content className="account-picker ui-popover" aria-label="Switch account" side="bottom" align="start" sideOffset={8} collisionPadding={12}
    // Focus the filter or the sheet itself, not the restart icon, so its tooltip doesn't pop on every open.
    onOpenAutoFocus={event => { event.preventDefault(); const sheet = event.target as HTMLElement; (sheet.querySelector<HTMLElement>(".picker-search input") ?? sheet).focus(); }}>
    {accounts.length > 5 && <label className="picker-search"><SearchIcon size={15} aria-hidden />
      <input type="text" value={query} onChange={event => setQuery(event.target.value)} placeholder="Filter accounts" aria-label="Filter accounts" />
    </label>}
    <div className="picker-list">
      {shown.map((account, index) => {
          const isCurrent = account.id === activeId;
          const rank = (isCurrent ? activeRank : null) ?? cards[account.id]?.rank ?? null;
          const slug = crestOf(rank);
          const [gameName, tag] = account.riotId?.split("#") ?? [];
          const name = account.nickname || gameName || account.name;
          const region = account.region ?? account.platform;
          // --i staggers the entrance (capped in CSS); --tier tints stripe, crest glow and tier name, unset when unranked.
          const style = { "--i": index, "--tier": slug ? `var(--ui-tier-${slug})` : undefined } as CSSProperties;
          const action = closed ? "Open League" : "Restart client";
          // The current account is a card that also holds its client action.
          return <div key={account.id} className={isCurrent ? "picker-current" : "picker-other"} style={style}><button type="button" className={`picker-row${isCurrent ? " is-current" : ""}`} disabled={disabled || isCurrent} onClick={() => onSwitch(account.id)}>
            <span className="picker-avatar">
              {account.iconDataUrl ? <img src={account.iconDataUrl} alt="" loading="lazy" /> : <span className="picker-initial">{name[0]?.toUpperCase()}</span>}
              {isCurrent && <span className="picker-check" aria-label="Current account"><CheckIcon size={12} /></span>}
            </span>
            <span className="picker-copy"><b>{name}</b>
              {(tag || region) && <span className="picker-meta">{[tag ? `#${tag}` : null, region].filter(Boolean).join(" · ")}</span>}
            </span>
            <span className="picker-tier">{slug && <RankCrest tier={slug} size={30} mode="desktop" />}<span>{rank ? rankLabel(rank) : "Unranked"}</span></span>
            {!isCurrent && <span className="picker-go" aria-hidden><SwitchIcon size={16} /></span>}
          </button>
          {isCurrent && <Tooltip label={action}><button type="button" className="picker-restart" disabled={disabled} onClick={onRestart}>
            <RefreshIcon size={17} aria-hidden /><span className="sr-only">{action}</span></button></Tooltip>}</div>;
      })}
      {!shown.length && <p className="picker-empty">{accounts.length ? "No matches." : "No saved accounts yet."}</p>}
    </div>
    <div className="picker-foot">
      <button type="button" onClick={onManage}><SettingsIcon size={18} aria-hidden />Manage accounts</button>
      <Tooltip label={addHint}><button type="button" className="picker-info" aria-label={addHint}><InfoIcon size={17} aria-hidden /></button></Tooltip>
    </div>
  </Popover.Content></Popover.Portal>;
}

/** Dims the page while a switch runs. The switch flow reports no stages, so the bar is an honest indeterminate one. */
export function SwitchOverlay({ account, name, restart }: { account: Account | undefined; name: string; restart?: boolean }) {
  return <div className="switch-overlay" role="status" aria-live="polite">
    <div className="switching-card ui-card is-raised">
      <div className="who">
        {account?.iconDataUrl ? <img src={account.iconDataUrl} alt="" /> : <span className="picker-initial">{name[0]?.toUpperCase()}</span>}
        <div><h2>{restart ? "Restarting client" : `Switching to ${name}`}</h2><p>{restart ? `Signing ${name} back in through Riot Client.` : "Signing in through Riot Client."}</p></div>
      </div>
      <div className="switch-bar" aria-hidden><i /></div>
    </div>
  </div>;
}
