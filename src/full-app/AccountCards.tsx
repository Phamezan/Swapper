import { DropdownMenu } from "radix-ui";
import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ArrowRight, Check, MoreHorizontal, Plus, ShieldCheck, Wrench } from "lucide-react";
import type { Account } from "../App";
import { Notice } from "./ui/States";

export type AccountCardsProps = {
  accounts: Account[]; activeId: string | null; switchingId: string | null; repairableId: string | null; busy: boolean;
  onSwitch: (id: string) => void; onAdd: () => void; onRename: (account: Account) => void;
  onRemove: (id: string) => void; onRepair: (id: string) => void; onError: (reason: unknown) => void;
};
export function AccountCards(props: AccountCardsProps) {
  const [notice, setNotice] = useState("");
  const shortcut = async (id: string) => {
    try { await invoke("create_account_shortcut", { id }); setNotice("Desktop shortcut created."); }
    catch (reason) { props.onError(reason); }
  };
  return <div className="fa-accounts dash-scope">
    {notice && <Notice>{notice}</Notice>}
    <div className="fa-launcher-intro"><span><ShieldCheck size={20} /> Saved sessions</span><p>Pick an account. Swapper takes care of the sign-in.</p><small>{props.accounts.length} saved · {props.activeId ? "1 active" : "No active session"}</small></div>
    <div className="fa-account-grid">{props.accounts.map(account => {
      const active = props.activeId === account.id;
      const switching = props.switchingId === account.id;
      const repair = props.repairableId === account.id;
      return <article className={`fa-card fa-account-card${active ? " is-active" : ""}`} key={account.id}>
        <div className="fa-account-top"><span className={`fa-account-state${active ? " is-active" : ""}`}>{active ? <><Check size={12} /> Active session</> : repair ? <><Wrench size={12} /> Needs repair</> : "Ready to launch"}</span>
          <DropdownMenu.Root><DropdownMenu.Trigger className="fa-icon-button" disabled={props.busy} aria-label={`Actions for ${account.name}`}><MoreHorizontal size={20} /></DropdownMenu.Trigger>
            <DropdownMenu.Portal><DropdownMenu.Content className="fa-account-menu" align="end" sideOffset={6}>
              <DropdownMenu.Item onSelect={() => props.onRename(account)}>Rename account</DropdownMenu.Item>
              <DropdownMenu.Item onSelect={() => props.onRepair(account.id)}>Repair saved session</DropdownMenu.Item>
              <DropdownMenu.Item onSelect={() => props.onAdd()}>Add / update account</DropdownMenu.Item>
              <DropdownMenu.Item onSelect={() => void shortcut(account.id)}>Create desktop shortcut</DropdownMenu.Item>
              <DropdownMenu.Separator /><DropdownMenu.Item className="is-danger" onSelect={() => props.onRemove(account.id)}>Remove account…</DropdownMenu.Item>
            </DropdownMenu.Content></DropdownMenu.Portal>
          </DropdownMenu.Root>
        </div>
        <div className="fa-account-identity">{account.iconDataUrl ? <img src={account.iconDataUrl} alt="" loading="lazy" /> : <span className="fa-account-initial">{account.name[0]?.toUpperCase()}</span>}<span><h2>{account.name}</h2><p>{account.riotId ?? "Riot ID unavailable"}</p></span></div>
        <div className="fa-account-meta"><span>Region<strong>{account.region ?? account.platform ?? "Unknown"}</strong></span><span>Session<strong>{switching ? "Switching…" : active ? "In use" : "Saved securely"}</strong></span></div>
        <button className={active ? "fa-secondary" : "fa-primary"} disabled={props.busy || active} onClick={() => props.onSwitch(account.id)}>{switching ? "Signing in…" : active ? "Currently playing" : "Switch account"}{!active && <ArrowRight size={16} />}</button>
      </article>;
    })}<button className="fa-add-account" disabled={props.busy} onClick={props.onAdd}><Plus size={30} /><strong>{props.accounts.length ? "Add another account" : "Save your first account"}</strong><span>Sign in once through Riot Client.<br />Keep your sessions ready here.</span></button></div>
  </div>;
}
