import { Fragment, useState } from "react";
import { Popover } from "radix-ui";
import { CheckIcon, ForwardIcon, RefreshIcon, SearchIcon, SettingsIcon, SwitchIcon } from "./icons";
import type { Account } from "../../App";
import { BrandIcon } from "../../components/BrandIcon";
import { RankCrest } from "../../runes/RankCrest";
import type { AccountCard, Snapshot } from "../types";
import "./shell.css";

const apexTiers = new Set(["MASTER", "GRANDMASTER", "CHALLENGER"]);
const title = (text: string) => text.charAt(0) + text.slice(1).toLowerCase();
const crestOf = (rank: Snapshot["rank"]) => rank ? rank.tier.toLowerCase().replace(/\s+/g, "") : null;
const rankLabel = (rank: NonNullable<Snapshot["rank"]>) => `${title(rank.tier)}${apexTiers.has(rank.tier) ? "" : ` ${rank.division}`}`;

export type PodiumIdentity = {
  name: string; tag: string | null; icon: string | null; level: number | null; region: string | null;
  rank: Snapshot["rank"]; unranked: boolean;
};

/** Sidebar identity, with a separate account selector anchored at its foot. */
export function Podium({ who, connected, open, crest, disabled, onOpenProfile }: {
  who: PodiumIdentity; connected: boolean; open: boolean; crest: string | null; disabled: boolean; onOpenProfile?: () => void;
}) {
  const { rank } = who;
  const lp = rank && !apexTiers.has(rank.tier) ? Math.max(0, Math.min(100, rank.lp)) : null;
  const label = `${who.name}${who.tag ? `#${who.tag}` : ""}${rank ? `, ${rankLabel(rank)}, ${rank.lp} LP` : ""}. ${connected ? "League Client connected." : "League Client not connected."}`;
  return <section className="profile-podium" aria-label={label}>
    <span className="podium-hero" aria-hidden />
    <button type="button" className="podium-band" onClick={onOpenProfile} disabled={!onOpenProfile} title="Open your profile">
      <span className="podium-portrait">
        {who.icon ? <img src={who.icon} alt="" /> : <BrandIcon size={40} />}
        {who.level != null && <span className="podium-level">Lv {who.level}</span>}
        <i className={`podium-dot${connected ? "" : " is-off"}`} aria-hidden />
      </span>
      <span className="podium-copy">
        <strong>{who.name}{who.tag && <span className="podium-tagpart">#{who.tag}</span>}</strong>
      </span>
    </button>
    <span className="podium-standing">
      <span className="podium-rank">
        {rank && crest && <RankCrest tier={crest} size={40} mode="desktop" />}
        <span className="podium-rank-copy">
          <b>{rank ? rankLabel(rank) : who.unranked ? "Unranked" : "Not signed in"}</b>
          {rank && <span className="podium-lptext"><b>{rank.lp} LP</b></span>}
        </span>
      </span>
      {lp != null && <span className="podium-lp" role="img" aria-label={`${rank!.lp} of 100 LP to the next division`}><i style={{ width: `${lp}%` }} /></span>}
    </span>
    <Popover.Trigger asChild>
      <button type="button" className="podium-switch" disabled={disabled}>
        <SwitchIcon size={17} aria-hidden /><span>Switch account</span>
        <span className={`podium-chev${open ? " is-open" : ""}`} aria-hidden><ForwardIcon size={16} /></span>
      </button>
    </Popover.Trigger>
  </section>;
}

/** Account rows keep identity and rank together; Popover owns placement and dismissal. */
export function AccountPicker({ accounts, activeId, activeRank, cards, disabled, onSwitch, onManage }: {
  accounts: Account[]; activeId: string | null; cards: Record<string, AccountCard>; disabled: boolean;
  activeRank: Snapshot["rank"]; onSwitch: (id: string) => void; onManage: () => void;
}) {
  const [query, setQuery] = useState("");
  const needle = query.trim().toLowerCase();
  const shown = accounts.filter(account => !needle || [account.name, account.nickname, account.riotId].some(value => value?.toLowerCase().includes(needle)));
  return <Popover.Portal><Popover.Content className="account-picker ui-popover" aria-label="Switch account" side="bottom" align="start" sideOffset={8} collisionPadding={12}>
    {accounts.length > 5 && <label className="picker-search"><SearchIcon size={15} aria-hidden />
      <input type="text" value={query} onChange={event => setQuery(event.target.value)} placeholder="Filter accounts" aria-label="Filter accounts" />
    </label>}
    <div className="picker-list">
      {shown.map(account => {
          const isCurrent = account.id === activeId;
          const rank = (isCurrent ? activeRank : null) ?? cards[account.id]?.rank ?? null;
          const slug = crestOf(rank);
          const [gameName, tag] = account.riotId?.split("#") ?? [];
          const name = account.nickname || gameName || account.name;
          const region = account.region ?? account.platform;
          return <Fragment key={account.id}><button type="button" className={`picker-row${isCurrent ? " is-current" : ""}`} disabled={disabled || isCurrent} onClick={() => onSwitch(account.id)}>
            {account.iconDataUrl ? <img src={account.iconDataUrl} alt="" loading="lazy" /> : <span className="picker-initial">{name[0]?.toUpperCase()}</span>}
            <span className="picker-copy"><b>{name}</b>
              <span className="picker-meta">
                {(tag || region) && <span>{[tag ? `#${tag}` : null, region].filter(Boolean).join(" · ")}</span>}
                <span className="picker-tier">{slug && <RankCrest tier={slug} size={16} mode="desktop" />}{rank ? rankLabel(rank) : "Unranked"}</span>
              </span>
            </span>
            {isCurrent && <span className="picker-check" aria-label="Current account"><CheckIcon size={16} /></span>}
          </button>
          {/* Switching to the signed-in account restarts the client with it (same as the tray flyout). */}
          {isCurrent && <button type="button" className="picker-restart" disabled={disabled} onClick={() => onSwitch(account.id)}><RefreshIcon size={15} aria-hidden />Restart client</button>}</Fragment>;
      })}
      {!shown.length && <p className="picker-empty">{accounts.length ? "No matches." : "No saved accounts yet."}</p>}
    </div>
    <div className="picker-foot">
      <p>Sign in through Riot Client to save another account.</p>
      <button type="button" onClick={onManage}><SettingsIcon size={18} aria-hidden />Manage accounts</button>
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
