import { invoke } from "@tauri-apps/api/core";
import { ArrowRight, ChevronRight, LoaderCircle, Pencil, Trash2, X } from "lucide-react";
import { AddAccountIcon, AddIcon, CheckIcon, RefreshIcon, SettingsIcon, WarningIcon } from "../full-app/ui/icons";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { RepairPanel } from "../accounts/RepairPanel";
import { CreateShortcutButton } from "../accounts/CreateShortcutButton";
import { UpdateBanner } from "../settings/UpdateSection";
import { LiveGameCompact } from "../full-app/LiveGameCompact";
import { PageHeader } from "../full-app/ui/PageHeader";
import { RiotClientIcon } from "../riotIcon";
import { IdentityCard } from "./IdentityCard";
import type { AppState, FullAppBridge } from "./types";
import type { AppController } from "./useAppController";

export function AccountsView({ controller, fullApp }: { controller: AppController; fullApp?: FullAppBridge }) {
  const {
    data, busy, switchingAccountId, setName, setEditing, setPendingRemove, repairableId,
    setDetectedPrompt, showError, showRemoteQr, dismissReconnect, navigate, isBusy, openAccountMenu,
    switchToAccount, startRepair, visibleDetectedPrompt, displayedActiveId, saveDetected,
  } = controller;
  const AccountsSurface = fullApp?.Accounts;
  return (
          <>
            {fullApp ? <PageHeader title="Accounts" /> : <div className="panel-heading">
              <div><p className="eyebrow">{data.accounts.length === 0 ? "Get started" : "Ready to play"}</p><h1>Accounts</h1></div>
              <button className="icon-button" aria-label="Settings" title="Settings" disabled={isBusy} onClick={() => navigate("settings")}><SettingsIcon size={19} /></button>
            </div>}
            {data.remote.lanReconnectNeeded && (
              <div className="reconnect-banner" role="status">
                <WarningIcon size={16} />
                <span>PC address changed. Reconnect your phone.</span>
                <button type="button" className="reconnect-banner-action" onClick={showRemoteQr}>Show QR</button>
                <button type="button" className="reconnect-dismiss" aria-label="Dismiss reconnect notice" onClick={dismissReconnect}><X size={14} /></button>
              </div>
            )}
            <UpdateBanner onOpenSettings={() => navigate("settings")} />
            {!fullApp && <LiveGameCompact />}
            {visibleDetectedPrompt && (
              <section className="detected-prompt">
                <strong className="detected-title">Signed-in account detected</strong>
                <IdentityCard identity={visibleDetectedPrompt} />
                <Button className="primary-action" disabled={isBusy} onClick={() => saveDetected(visibleDetectedPrompt, "save-detected", () => setDetectedPrompt(null))}>
                  {busy === "save-detected" ? <LoaderCircle className="spin" size={16} /> : <CheckIcon size={16} />} Save Account
                </Button>
              </section>
            )}
            {AccountsSurface ? <AccountsSurface accounts={data.accounts} activeId={displayedActiveId} switchingId={switchingAccountId} repairableId={repairableId} busy={isBusy}
              onSwitch={id => void switchToAccount(id)} onAdd={() => navigate("add")} onRepair={startRepair} onError={showError}
              onRename={account => { navigate("edit"); setEditing(account.id); setName(account.nickname ?? ""); }}
              onRemove={id => { navigate("remove"); setPendingRemove(id); }} /> : <div className="account-list">
              {data.accounts.length === 0 ? (
                <div className="empty-state">
                  <div className="empty-symbol"><AddIcon size={22} /></div>
                  <h2>No accounts yet</h2>
                  <p>Sign in through Riot Client, then save it here.</p>
                  <Button disabled={isBusy} onClick={() => navigate("add")}>Add first account <ArrowRight size={15} /></Button>
                </div>
              ) : data.accounts.map((account, index) => {
                const isActive = displayedActiveId === account.id;
                const isThisSwitching = switchingAccountId === account.id;
                return (
                  <button
                    key={account.id}
                    className={`account-row ${isActive ? "is-active" : ""}`}
                    disabled={isBusy}
                    onClick={() => switchToAccount(account.id)}
                    onContextMenu={(event) => { event.preventDefault(); openAccountMenu(account.id, event.clientX, event.clientY); }}
                    onKeyDown={(event) => {
                      if (event.key === "ContextMenu" || (event.shiftKey && event.key === "F10")) {
                        event.preventDefault();
                        const rect = event.currentTarget.getBoundingClientRect();
                        openAccountMenu(account.id, rect.left + 12, rect.bottom - 8);
                      }
                    }}
                  >
                    {account.iconDataUrl
                      ? <img className="account-avatar account-avatar-image" src={account.iconDataUrl} alt="" />
                      : <span className="account-avatar">{account.name.slice(0, 1).toUpperCase() || index + 1}</span>}
                    <span className="account-copy">
                      <strong>{account.name}</strong>
                      <small>
                        {isThisSwitching
                          ? "Switching…"
                          : isActive
                            ? "Active"
                            : repairableId === account.id
                              ? "Session needs repair"
                              : "Click to switch"}
                        {account.region ? ` · ${account.region}` : ""}
                      </small>
                    </span>
                    {isThisSwitching || busy === `switch-${account.id}` ? (
                      <LoaderCircle className="spin" size={20} />
                    ) : isActive ? (
                      <span className="active-indicator"><CheckIcon size={14} /></span>
                    ) : (
                      <ChevronRight size={20} className="row-chevron" />
                    )}
                  </button>
                );
              })}
            </div>}
            {!AccountsSurface && data.accounts.length > 0 && <Button variant="outline" className="add-inline" disabled={isBusy} onClick={() => navigate("add")}><AddAccountIcon size={17} /> Add account</Button>}
          </>
  );
}

export function AccountForms({ controller }: { controller: AppController; fullApp?: FullAppBridge }) {
  const {
    data, view, busy, setError, name, setName, editing, setEditing, pendingRemove, setPendingRemove,
    repairingId, setRepairingId, addStage, setAddStage, detect, setDetectRound, sync, showError, action,
    navigate, conflictingSavedAccount, saveDetected,
  } = controller;
  return (
          <>
            {view === "repair" && <RepairPanel
              account={data.accounts.find((account) => account.id === repairingId)}
              onDone={(next) => { setRepairingId(null); sync(next); navigate("accounts"); }}
              onCancel={() => { setRepairingId(null); navigate("accounts"); }}
            />}

            {view === "add" && <div className="form-content add-flow">
              <div className="flow-progress" aria-label={`Step ${addStage === "signIn" ? 1 : 2} of 2`}>
                <span className={`flow-progress-item ${addStage === "signIn" ? "is-active" : "is-complete"}`}>01 · Sign in</span>
                <span className="flow-divider" />
                <span className={`flow-progress-item ${addStage === "identify" ? "is-active" : ""}`}>02 · Save</span>
              </div>
              {addStage === "signIn" ? <section className="add-stage">
                <span className="step-number">01</span>
                <h2>Sign in to Riot Client</h2>
                <p>Turn on “Stay signed in” in Riot Client. Swapper never asks for your password.</p>
                <Button className="primary-action riot-launch" disabled={busy !== null} onClick={() => action("launch", async () => {
                  return data.accounts.length > 0 ? invoke<AppState>("begin_add") : invoke<void>("open_riot");
                }, () => setAddStage("identify"))}>
                  {busy === "launch" ? <LoaderCircle className="spin" size={16} /> : <RiotClientIcon />}
                  {data.accounts.length > 0 ? "Sign in to another account" : "Open Riot Client"}
                </Button>
                <button className="text-action" onClick={() => setAddStage("identify")}>Already signed in? Detect account</button>
              </section> : <section className="add-stage">
                <span className="step-number">02</span>
                {detect.phase === "found" && conflictingSavedAccount ? <>
                  <h2>Account name already saved</h2>
                  <p>A saved account has this Riot ID but a different identifier. Swapper can’t safely save a copy or update it.</p>
                  <IdentityCard identity={detect.identity} />
                  <button className="text-action" onClick={() => navigate("accounts")}>Back to accounts</button>
                </> : detect.phase === "found" && detect.savedAccountId === null ? <>
                  <h2>Account detected</h2>
                  <IdentityCard identity={detect.identity} />
                  <Button className="primary-action" disabled={busy !== null} onClick={() => saveDetected(detect.identity, "save", () => navigate("accounts"))}>
                    {busy === "save" ? <LoaderCircle className="spin" size={16} /> : <CheckIcon size={16} />} Save Account
                  </Button>
                  <button className="text-action" onClick={() => setAddStage("signIn")}>Use a different account</button>
                </> : detect.phase === "found" ? <>
                  <h2>Account already saved</h2>
                  <p>This Riot account is already saved in Swapper.</p>
                  <IdentityCard identity={detect.identity} />
                  <Button className="primary-action" disabled={busy !== null} onClick={() => saveDetected(detect.identity, "update", () => navigate("accounts"))}>
                    {busy === "update" ? <LoaderCircle className="spin" size={16} /> : <RefreshIcon size={16} />} Update Saved Account
                  </Button>
                  <button className="text-action" onClick={() => setAddStage("signIn")}>Use a different account</button>
                </> : detect.phase === "failed" ? <>
                  <h2>Could not read account identity</h2>
                  <p>Open Riot Client, sign in, then try again.</p>
                  <Button className="primary-action" disabled={busy !== null} onClick={() => { setError(null); setDetectRound((round) => round + 1); }}>
                    <RefreshIcon size={16} /> Try Again
                  </Button>
                  <button className="text-action" onClick={() => setAddStage("signIn")}>Back to sign-in</button>
                </> : detect.phase === "waiting" && detect.reason === "noClient" ? <>
                  <h2>Open Riot Client</h2>
                  <p>Sign in with “Stay signed in”. Swapper detects the account automatically.</p>
                  <div className="detect-status"><LoaderCircle className="spin" size={14} /> Looking for Riot Client…</div>
                </> : <>
                  <h2>Waiting for Riot sign-in…</h2>
                  <p>Keep Riot Client open. Swapper detects the account automatically.</p>
                  <div className="detect-status"><LoaderCircle className="spin" size={14} /> Watching for your account…</div>
                </>}
              </section>}
            </div>}

            {(view === "edit" || view === "remove") && <div className="manage-list">
              {data.accounts.length === 0 && <p className="support-text">No saved accounts.</p>}
              {data.accounts.map((account) => <div className="manage-row" key={account.id}>
                {account.iconDataUrl
                  ? <img className="manage-avatar account-avatar-image" src={account.iconDataUrl} alt="" />
                  : <div className="manage-avatar">{account.name.slice(0, 1).toUpperCase()}</div>}
                <div className="manage-copy">
                  {editing === account.id
                    ? <Input autoFocus value={name} maxLength={40} placeholder={account.riotId ?? account.name} onChange={(e) => setName(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter") action("nickname", () => invoke<AppState>("set_nickname", { id: account.id, name }), () => setEditing(null)); }} />
                    : <strong>{account.name}</strong>}
                  {account.nickname && account.riotId && <small>{account.riotId}</small>}
                </div>
                {view === "edit" && editing !== account.id && (
                  <CreateShortcutButton accountId={account.id} accountName={account.name} disabled={busy !== null} onError={showError} />
                )}
                {view === "edit" ? editing === account.id
                  ? <button className="icon-button" aria-label={`Save nickname for ${account.name}`} onClick={() => action("nickname", () => invoke<AppState>("set_nickname", { id: account.id, name }), () => setEditing(null))}><CheckIcon size={17} /></button>
                  : <button className="icon-button" aria-label={`Set a nickname for ${account.name}`} onClick={() => { setEditing(account.id); setName(account.nickname ?? ""); }}><Pencil size={16} /></button>
                  : <button className="icon-button danger-icon" aria-label={`Remove ${account.name}`} onClick={() => setPendingRemove(account.id)}><Trash2 size={16} /></button>}
                {pendingRemove === account.id && view === "remove" && <div className="remove-confirm"><span>Remove this saved session?</span><Button size="sm" variant="destructive" disabled={busy !== null} onClick={() => action("remove", () => invoke<AppState>("remove_account", { id: account.id }), () => setPendingRemove(null))}>Remove</Button><Button size="sm" variant="ghost" onClick={() => setPendingRemove(null)}>Cancel</Button></div>}
              </div>)}
            </div>}

          </>
  );
}

export function AccountMenu({ controller }: { controller: AppController; fullApp?: FullAppBridge }) {
  const {
    view, setName, setEditing, setPendingRemove, accountMenu, setAccountMenu, navigate, menuAccount,
    switchToAccount,
  } = controller;
  return (
accountMenu && menuAccount && view === "accounts" && <div
        className="account-context-menu"
        role="menu"
        aria-label={`Actions for ${menuAccount.name}`}
        style={{ left: accountMenu.x, top: accountMenu.y }}
      >
        <button role="menuitem" autoFocus onClick={() => { setAccountMenu(null); void switchToAccount(menuAccount.id); }}>Switch to account</button>
        <button role="menuitem" onClick={() => { navigate("edit"); setEditing(menuAccount.id); setName(menuAccount.nickname ?? ""); }}>Edit nickname</button>
        <button role="menuitem" className="is-danger" onClick={() => { navigate("remove"); setPendingRemove(menuAccount.id); }}>Remove account</button>
        <span className="account-menu-divider" />
        <button role="menuitem" onClick={() => navigate("add")}>Add account</button>
      </div>
  );
}

