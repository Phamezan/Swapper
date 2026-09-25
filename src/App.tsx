import { useEffect, useRef, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { QRCodeSVG } from "qrcode.react";
import { open as browseForFile } from "@tauri-apps/plugin-dialog";
import {
  ArrowLeft, ArrowRight, Check, ChevronRight, CircleAlert, Info, LoaderCircle,
  Pencil, Plus, QrCode, RefreshCw, Settings2, Trash2, X,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import "./App.css";

type Account = {
  id: string;
  name: string;
  riotId: string | null;
  nickname: string | null;
  platform: string | null;
  region: string | null;
  profileIconId: number | null;
  iconDataUrl: string | null;
};
type DetectedIdentity = {
  puuid: string;
  gameName: string;
  tagLine: string;
  platform: string | null;
  region: string | null;
  profileIconId: number | null;
  iconDataUrl: string | null;
};
type DetectOutcome =
  | { state: "identified"; identity: DetectedIdentity; savedAccountId: string | null }
  | { state: "notRunning" }
  | { state: "notReady" }
  | { state: "unavailable" };
type DetectState =
  | { phase: "idle" }
  | { phase: "waiting"; reason: "noClient" | "signingIn" }
  | { phase: "found"; identity: DetectedIdentity; savedAccountId: string | null }
  | { phase: "failed" };
type RemoteState =
  | "disabled"
  | "starting"
  | "notInstalled"
  | "disconnected"
  | "available"
  | "failed";
type RemoteStatus = {
  enabled: boolean;
  state: RemoteState;
  address: string | null;
  message: string | null;
  tailscaleInstalled: boolean;
  tailscaleRunning: boolean;
  dnsName: string | null;
  leagueRunning: boolean;
  lcuConnected: boolean;
};
type AppState = {
  accounts: Account[];
  activeId: string | null;
  useDeceive: boolean;
  riotExe: string | null;
  riotDetected: boolean;
  deceiveDetected: boolean;
  remote: RemoteStatus;
};
type View = "accounts" | "add" | "edit" | "remove" | "settings";

const emptyRemote: RemoteStatus = {
  enabled: false, state: "disabled", address: null, message: null,
  tailscaleInstalled: false, tailscaleRunning: false, dnsName: null,
  leagueRunning: false, lcuConnected: false,
};

const empty: AppState = {
  accounts: [], activeId: null, useDeceive: false,
  riotExe: null, riotDetected: false, deceiveDetected: false,
  remote: emptyRemote,
};

const wait = (ms: number) =>
  new Promise<void>((resolve) => {
    window.setTimeout(resolve, ms);
  });

function riotIdOf(identity: DetectedIdentity) {
  return `${identity.gameName}#${identity.tagLine}`;
}

function IdentityCard({ identity }: { identity: DetectedIdentity }) {
  return (
    <div className="identity-card">
      {identity.iconDataUrl
        ? <img className="identity-icon" src={identity.iconDataUrl} alt="" />
        : <span className="identity-icon identity-avatar">{identity.gameName.slice(0, 1).toUpperCase()}</span>}
      <span className="identity-copy">
        <strong>{riotIdOf(identity)}</strong>
        {identity.region && <small>{identity.region}</small>}
      </span>
    </div>
  );
}

function App() {
  const [data, setData] = useState<AppState>(empty);
  const [view, setView] = useState<View>("accounts");
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [editing, setEditing] = useState<string | null>(null);
  const [pendingRemove, setPendingRemove] = useState<string | null>(null);
  const [accountMenu, setAccountMenu] = useState<{ id: string; x: number; y: number } | null>(null);
  const [useDeceive, setUseDeceive] = useState(false);
  const [riotExe, setRiotExe] = useState("");
  const [addStage, setAddStage] = useState<"signIn" | "identify">("signIn");
  const [detect, setDetect] = useState<DetectState>({ phase: "idle" });
  const [detectRound, setDetectRound] = useState(0);
  const [detectedPrompt, setDetectedPrompt] = useState<DetectedIdentity | null>(null);
  const detectionEpoch = useRef(0);
  const [copied, setCopied] = useState(false);
  const [showRemoteQr, setShowRemoteQr] = useState(false);
  const [openInfo, setOpenInfo] = useState<string | null>(null);
  const [pathMode, setPathMode] = useState<"auto" | "manual" | null>(null);
  const native = isTauri();

  useEffect(() => {
    if (!native) return;
    invoke<AppState>("get_state").then(sync).catch(showError);
    const listener = listen<string>("navigate", ({ payload }) => {
      if (["accounts", "add", "edit", "remove", "settings"].includes(payload)) {
        setView(payload as View);
        setAccountMenu(null);
        setError(null);
        if (payload === "add") {
          setName("");
          setAddStage("signIn");
          setDetect({ phase: "idle" });
        }
      }
      invoke<AppState>("get_state").then(sync).catch(showError);
    });
    return () => { listener.then((unlisten) => unlisten()); };
  }, [native]);

  function sync(next: AppState) {
    setData(next);
    setUseDeceive(next.useDeceive);
    setRiotExe(next.riotExe ?? "");
  }

  useEffect(() => {
    if (!native || view !== "settings") return;
    let live = true;
    invoke<RemoteStatus>("probe_remote")
      .then((remote) => {
        if (live) setData((prev) => ({ ...prev, remote }));
      })
      .catch(showError);
    return () => { live = false; };
  }, [native, view]);

  // Passive detection while the Add Account view waits for a Riot login.
  // Only runs while this stage is open; stops as soon as an account appears.
  useEffect(() => {
    if (view !== "add" || addStage !== "identify") return;
    if (!native) {
      setDetect({ phase: "failed" });
      return;
    }
    setDetect({ phase: "waiting", reason: "signingIn" });
    let cancelled = false;
    (async () => {
      while (!cancelled) {
        let outcome: DetectOutcome;
        try {
          outcome = await invoke<DetectOutcome>("detect_account_identity");
        } catch {
          outcome = { state: "unavailable" };
        }
        if (cancelled) return;
        if (outcome.state === "identified") {
          setDetect({
            phase: "found",
            identity: outcome.identity,
            savedAccountId: outcome.savedAccountId,
          });
          invoke<AppState>("get_state").then((next) => { if (!cancelled) sync(next); }).catch(() => {});
          return;
        }
        if (outcome.state === "unavailable") {
          setDetect({ phase: "failed" });
          return;
        }
        setDetect({
          phase: "waiting",
          reason: outcome.state === "notRunning" ? "noClient" : "signingIn",
        });
        await wait(2000);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [view, addStage, native, detectRound]);

  // Short detection burst when the accounts view opens (app start or return),
  // so an already signed-in unsaved account can be saved without typing.
  // Bounded on purpose: no polling while the flyout is idle.
  useEffect(() => {
    if (!native || view !== "accounts") {
      setDetectedPrompt(null);
      return;
    }
    let cancelled = false;
    const epoch = detectionEpoch.current;
    setDetectedPrompt(null);
    (async () => {
      for (let attempt = 0; attempt < 4; attempt += 1) {
        if (attempt > 0) await wait(2000);
        if (cancelled || epoch !== detectionEpoch.current) return;
        let outcome: DetectOutcome;
        try {
          outcome = await invoke<DetectOutcome>("detect_account_identity");
        } catch {
          return;
        }
        if (cancelled || epoch !== detectionEpoch.current) return;
        if (outcome.state === "identified") {
          // The backend may have just refreshed the active account's metadata.
          invoke<AppState>("get_state").then((next) => { if (!cancelled && epoch === detectionEpoch.current) sync(next); }).catch(() => {});
          if (!outcome.savedAccountId) setDetectedPrompt(outcome.identity);
          return;
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [native, view]);

  function showError(reason: unknown) {
    setError(String(reason));
  }

  async function setRemote(enabled: boolean) {
    if (!native) {
      setError("Remote Control is available in the Windows tray app. Start it with npm run tauri dev.");
      return;
    }
    const previous = data.remote;
    if (!enabled) setShowRemoteQr(false);
    setCopied(false);
    setData((prev) => ({
      ...prev,
      remote: { ...prev.remote, enabled, state: enabled ? "starting" : "disabled" },
    }));
    try {
      const remote = await invoke<RemoteStatus>("set_remote_enabled", { enabled });
      setData((prev) => ({ ...prev, remote }));
    } catch (reason) {
      setData((prev) => ({ ...prev, remote: previous }));
      showError(reason);
    }
  }

  async function copyRemoteAddress() {
    const address = data.remote.address;
    if (!address) return;
    try {
      await navigator.clipboard.writeText(address);
    } catch {
      const field = document.createElement("textarea");
      field.value = address;
      field.style.position = "fixed";
      field.style.opacity = "0";
      document.body.appendChild(field);
      field.select();
      document.execCommand("copy");
      field.remove();
    }
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1500);
  }

  async function action(key: string, fn: () => Promise<AppState | void>, after?: () => void) {
    if (!native) {
      setError("Account actions are available in the Windows tray app. Start it with npm run tauri dev.");
      return;
    }
    setBusy(key);
    setError(null);
    try {
      const result = await fn();
      if (result) sync(result);
      after?.();
    } catch (reason) {
      showError(reason);
    } finally {
      setBusy(null);
    }
  }

  const navigate = (next: View) => {
    if (native) void invoke("set_add_mode", { enabled: next === "add" }).catch(showError);
    setView(next);
    setAccountMenu(null);
    setError(null);
    setEditing(null);
    setPendingRemove(null);
    setShowRemoteQr(false);
    if (next === "add") {
      setName("");
      setAddStage("signIn");
      setDetect({ phase: "idle" });
    }
  };
  useEffect(() => {
    if (!accountMenu) return;
    const dismiss = (event: PointerEvent) => {
      if (!(event.target as Element).closest(".account-context-menu")) setAccountMenu(null);
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") setAccountMenu(null);
    };
    document.addEventListener("pointerdown", dismiss);
    document.addEventListener("keydown", escape);
    return () => {
      document.removeEventListener("pointerdown", dismiss);
      document.removeEventListener("keydown", escape);
    };
  }, [accountMenu]);
  const openAccountMenu = (id: string, x: number, y: number) => {
    if (busy) return;
    setAccountMenu({
      id,
      x: Math.max(8, Math.min(x, window.innerWidth - 192)),
      y: Math.max(8, Math.min(y, window.innerHeight - 164)),
    });
  };
  const menuAccount = data.accounts.find((account) => account.id === accountMenu?.id);
  const switchToAccount = (id: string) => action(`switch-${id}`, () => invoke<AppState>("switch_account", { id }), () => {
    detectionEpoch.current += 1;
    setDetectedPrompt(null);
    void invoke("hide_flyout");
  });
  const active = data.accounts.find((account) => account.id === data.activeId);
  const savedWithRiotId = (identity: DetectedIdentity) => data.accounts.find((account) =>
    account.riotId?.toLowerCase() === riotIdOf(identity).toLowerCase());
  // The display ID is enough to suppress a redundant save prompt, but a
  // different PUUID must never be treated as proof that this is the same login.
  const matchingDetectedAccount = detectedPrompt ? savedWithRiotId(detectedPrompt) : null;
  const visibleDetectedPrompt = detectedPrompt && !matchingDetectedAccount ? detectedPrompt : null;
  const displayedActiveId = detectedPrompt ? null : data.activeId;
  const footerUnsaved = Boolean(detectedPrompt && !matchingDetectedAccount);
  const footerText = footerUnsaved
    ? "Unsaved account detected"
    : active
      ? `Logged in as ${active.name}`
      : "Not logged in";
  const footerLoggedIn = !footerUnsaved && Boolean(active);
  const conflictingSavedAccount = detect.phase === "found" && detect.savedAccountId === null
    ? savedWithRiotId(detect.identity) : null;
  const saveDetected = (identity: DetectedIdentity, key: string, after?: () => void) =>
    action(key, () => invoke<AppState>("complete_add", { puuid: identity.puuid }), after);

  const manualPath = pathMode === null ? Boolean(data.riotExe) : pathMode === "manual";
  const configuredPath = () => (manualPath ? riotExe.trim() || null : null);

  // Settings apply immediately. The backend validates and persists, then a state
  // refresh pulls local edits back in line — or rolls them back when it rejects.
  const applySettings = async (patch: { useDeceive: boolean; riotExe: string | null }) => {
    await action("settings", async () => {
      await invoke("save_settings", patch);
    });
    if (native) void invoke<AppState>("get_state").then(sync).catch(showError);
  };

  const browseRiotPath = async () => {
    if (!native) return;
    void invoke("set_add_mode", { enabled: true }).catch(showError);
    try {
      const picked = await browseForFile({
        multiple: false,
        filters: [{ name: "Riot Client", extensions: ["exe"] }],
      });
      if (typeof picked === "string" && picked.trim()) {
        setPathMode("manual");
        setRiotExe(picked);
        await applySettings({ useDeceive, riotExe: picked });
      }
    } catch (reason) {
      showError(reason);
    } finally {
      void invoke("set_add_mode", { enabled: false }).catch(showError);
    }
  };

  return (
    <div className="shell dark">
      <header className="topbar">
        <div className="brand-mark"><ArrowRight size={16} strokeWidth={2.4} /><ArrowLeft size={16} strokeWidth={2.4} /></div>
        <div className="brand-copy"><strong>Swapper</strong><span>RIOT ACCOUNTS</span></div>
        <button className="icon-button close-button" aria-label="Hide Swapper" onClick={() => { if (native) void invoke("hide_flyout"); }}> <X size={16} /> </button>
      </header>

      <main className="main-panel">
        {view === "accounts" ? (
          <>
            <div className="panel-heading">
              <div><p className="eyebrow">{data.accounts.length === 0 ? "GET STARTED" : "READY TO PLAY"}</p><h1>Your accounts</h1></div>
              <button className="icon-button" aria-label="Settings" onClick={() => navigate("settings")}><Settings2 size={19} /></button>
            </div>
            {visibleDetectedPrompt && (
              <section className="detected-prompt">
                <strong className="detected-title">Signed-in account detected</strong>
                <IdentityCard identity={visibleDetectedPrompt} />
                <Button className="primary-action" disabled={busy !== null} onClick={() => saveDetected(visibleDetectedPrompt, "save-detected", () => setDetectedPrompt(null))}>
                  {busy === "save-detected" ? <LoaderCircle className="spin" size={16} /> : <Check size={16} />} Save Account
                </Button>
              </section>
            )}
            <div className="account-list">
              {data.accounts.length === 0 ? (
                <div className="empty-state">
                  <div className="empty-symbol"><Plus size={22} /></div>
                  <h2>No accounts yet</h2>
                  <p>Sign in through Riot Client once, then save that session here.</p>
                  <Button onClick={() => navigate("add")}>Add first account <ArrowRight size={15} /></Button>
                </div>
              ) : data.accounts.map((account, index) => {
                const isActive = displayedActiveId === account.id;
                return (
                  <button
                    key={account.id}
                    className={`account-row ${isActive ? "is-active" : ""}`}
                    disabled={busy !== null}
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
                      <small>{isActive ? "Current session" : "Click to switch"}{account.region ? ` · ${account.region}` : ""}</small>
                    </span>
                    {busy === `switch-${account.id}` ? <LoaderCircle className="spin" size={20} /> : isActive ? <span className="active-indicator"><Check size={14} /></span> : <ChevronRight size={20} className="row-chevron" />}
                  </button>
                );
              })}
            </div>
            {data.accounts.length > 0 && <Button className="add-inline" onClick={() => navigate("add")}><Plus size={17} /> Add account</Button>}
          </>
        ) : (
          <>
            <div className="panel-heading compact">
              <button className="icon-button back-button" aria-label="Back to accounts" onClick={() => navigate("accounts")}><ArrowLeft size={17} /></button>
              <div><p className="eyebrow">SWAPPER</p><h1>{view === "add" ? "Add account" : view === "settings" ? "Settings" : view === "edit" ? "Edit accounts" : "Remove account"}</h1></div>
            </div>

            {view === "add" && <div className="form-content add-flow">
              <div className="flow-progress" aria-label={`Step ${addStage === "signIn" ? 1 : 2} of 2`}>
                <span className={`flow-progress-item ${addStage === "signIn" ? "is-active" : "is-complete"}`}>01 · Sign in</span>
                <span className="flow-divider" />
                <span className={`flow-progress-item ${addStage === "identify" ? "is-active" : ""}`}>02 · Save</span>
              </div>
              {addStage === "signIn" ? <section className="add-stage">
                <span className="step-number">01</span>
                <h2>Sign in to Riot Client</h2>
                <p>Use the official Riot Client and turn on “Stay signed in”. Swapper never asks for your password. League does not need to be open.</p>
                <Button className="primary-action" disabled={busy !== null} onClick={() => action("launch", async () => {
                  await invoke("set_add_mode", { enabled: true });
                  return data.accounts.length > 0 ? invoke<AppState>("begin_add") : invoke<void>("open_riot");
                }, () => setAddStage("identify"))}>
                  {busy === "launch" ? <LoaderCircle className="spin" size={16} /> : <ArrowRight size={16} />}
                  {data.accounts.length > 0 ? "Sign in to another account" : "Open Riot Client"}
                </Button>
                <button className="text-action" onClick={() => setAddStage("identify")}>Already signed in? Detect account</button>
              </section> : <section className="add-stage">
                <span className="step-number">02</span>
                {detect.phase === "found" && conflictingSavedAccount ? <>
                  <h2>Account name already saved</h2>
                  <p>A saved account has this Riot ID, but its account identifier differs. Swapper cannot safely save another copy or update the saved session automatically.</p>
                  <IdentityCard identity={detect.identity} />
                  <button className="text-action" onClick={() => navigate("accounts")}>Back to accounts</button>
                </> : detect.phase === "found" && detect.savedAccountId === null ? <>
                  <h2>Account detected</h2>
                  <IdentityCard identity={detect.identity} />
                  <Button className="primary-action" disabled={busy !== null} onClick={() => saveDetected(detect.identity, "save", () => navigate("accounts"))}>
                    {busy === "save" ? <LoaderCircle className="spin" size={16} /> : <Check size={16} />} Save Account
                  </Button>
                  <button className="text-action" onClick={() => setAddStage("signIn")}>Use a different account</button>
                </> : detect.phase === "found" ? <>
                  <h2>Account already saved</h2>
                  <p>This Riot account is already saved in Swapper.</p>
                  <IdentityCard identity={detect.identity} />
                  <Button className="primary-action" disabled={busy !== null} onClick={() => saveDetected(detect.identity, "update", () => navigate("accounts"))}>
                    {busy === "update" ? <LoaderCircle className="spin" size={16} /> : <RefreshCw size={16} />} Update Saved Account
                  </Button>
                  <button className="text-action" onClick={() => setAddStage("signIn")}>Use a different account</button>
                </> : detect.phase === "failed" ? <>
                  <h2>Could not read account identity</h2>
                  <p>Make sure Riot Client is open and signed in, then try again.</p>
                  <Button className="primary-action" disabled={busy !== null} onClick={() => { setError(null); setDetectRound((round) => round + 1); }}>
                    <RefreshCw size={16} /> Try Again
                  </Button>
                  <button className="text-action" onClick={() => setAddStage("signIn")}>Back to sign-in</button>
                </> : detect.phase === "waiting" && detect.reason === "noClient" ? <>
                  <h2>Open Riot Client</h2>
                  <p>Sign in to Riot Client with “Stay signed in”. Swapper will detect your account automatically.</p>
                  <div className="detect-status"><LoaderCircle className="spin" size={14} /> Looking for Riot Client…</div>
                </> : <>
                  <h2>Waiting for Riot sign-in…</h2>
                  <p>Keep Riot Client open while it finishes signing in. Swapper will detect your account automatically.</p>
                  <div className="detect-status"><LoaderCircle className="spin" size={14} /> Watching for your account…</div>
                </>}
              </section>}
            </div>}

            {(view === "edit" || view === "remove") && <div className="manage-list">
              {data.accounts.length === 0 && <p className="support-text">There are no saved accounts yet.</p>}
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
                {view === "edit" ? editing === account.id
                  ? <button className="icon-button" aria-label={`Save nickname for ${account.name}`} onClick={() => action("nickname", () => invoke<AppState>("set_nickname", { id: account.id, name }), () => setEditing(null))}><Check size={17} /></button>
                  : <button className="icon-button" aria-label={`Set a nickname for ${account.name}`} onClick={() => { setEditing(account.id); setName(account.nickname ?? ""); }}><Pencil size={16} /></button>
                  : <button className="icon-button danger-icon" aria-label={`Remove ${account.name}`} onClick={() => setPendingRemove(account.id)}><Trash2 size={16} /></button>}
                {pendingRemove === account.id && view === "remove" && <div className="remove-confirm"><span>Remove this saved session?</span><Button size="sm" variant="destructive" disabled={busy !== null} onClick={() => action("remove", () => invoke<AppState>("remove_account", { id: account.id }), () => setPendingRemove(null))}>Remove</Button><Button size="sm" variant="ghost" onClick={() => setPendingRemove(null)}>Cancel</Button></div>}
              </div>)}
            </div>}

            {view === "settings" && <div className="settings-content">
              <div className="setting-row">
                <div className="setting-copy">
                  <strong>Launch through Deceive</strong>
                  <button className="info-button" aria-label="About Launch through Deceive" aria-expanded={openInfo === "deceive"} onClick={() => setOpenInfo(openInfo === "deceive" ? null : "deceive")}><Info size={13} /></button>
                  {openInfo === "deceive" && <p>Start League with Deceive’s offline presence. Included with Swapper.</p>}
                </div>
                <Switch checked={useDeceive} onCheckedChange={(checked) => void applySettings({ useDeceive: checked, riotExe: configuredPath() })} aria-label="Launch through Deceive" />
              </div>

              <p className="field-label">RIOT CLIENT PATH</p>
              <div className="segmented" role="group" aria-label="Riot Client path">
                <button type="button" className={manualPath ? "" : "is-on"} aria-pressed={!manualPath} onClick={() => { setPathMode("auto"); setRiotExe(""); void applySettings({ useDeceive, riotExe: null }); }}>Auto</button>
                <button type="button" className={manualPath ? "is-on" : ""} aria-pressed={manualPath} onClick={() => setPathMode("manual")}>Manual</button>
              </div>
              {manualPath && (
                <div className="path-input">
                  <Input id="riot-path" placeholder="Path to RiotClientServices.exe" value={riotExe} onChange={(e) => setRiotExe(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter") e.currentTarget.blur(); }} onBlur={() => {
                    const value = riotExe.trim();
                    if (!value) { setPathMode("auto"); void applySettings({ useDeceive, riotExe: null }); return; }
                    void applySettings({ useDeceive, riotExe: value });
                  }} />
                  <Button variant="outline" onMouseDown={(e) => e.preventDefault()} onClick={() => void browseRiotPath()}>Browse…</Button>
                </div>
              )}

              <div className="setting-row remote-setting">
                <div className="setting-copy">
                  <strong>Remote Control</strong>
                  <button className="info-button" aria-label="About Remote Control" aria-expanded={openInfo === "remote"} onClick={() => setOpenInfo(openInfo === "remote" ? null : "remote")}><Info size={13} /></button>
                  {openInfo === "remote" && <p>Control Swapper from your phone through your Tailscale network. Devices on your tailnet only — no pairing codes.</p>}
                </div>
                <Switch checked={data.remote.enabled} onCheckedChange={setRemote} aria-label="Remote Control" />
              </div>
              {data.remote.state === "starting" && <div className="setting-status"><LoaderCircle className="spin" size={13} /> Starting the remote service…</div>}
              {data.remote.state === "available" && data.remote.address && (
                <>
                  <div className="remote-address">
                    <Input readOnly value={data.remote.address} aria-label="Remote address" onFocus={(e) => e.currentTarget.select()} />
                    <Button variant="outline" disabled={copied} onClick={() => void copyRemoteAddress()}>{copied ? "Copied" : "Copy"}</Button>
                    <Button variant="outline" className="remote-qr-toggle" aria-label={showRemoteQr ? "Hide remote QR code" : "Show remote QR code"} aria-controls="remote-qr-panel" aria-expanded={showRemoteQr} onClick={() => setShowRemoteQr((shown) => !shown)}><QrCode size={15} /> QR</Button>
                  </div>
                  {showRemoteQr && <div id="remote-qr-panel" className="remote-qr-panel">
                    <div className="remote-qr-image"><QRCodeSVG value={data.remote.address} size={176} level="M" marginSize={4} bgColor="#ffffff" fgColor="#18181b" title="Tailscale remote address QR code" /></div>
                    <p>Scan with a phone connected to your Tailscale network.</p>
                  </div>}
                </>
              )}
              {(data.remote.state === "notInstalled" || data.remote.state === "disconnected" || data.remote.state === "failed") && data.remote.message && (
                <div className="setting-status remote-error"><CircleAlert size={13} /> {data.remote.message}</div>
              )}
            </div>}
          </>
        )}
      </main>
      {accountMenu && menuAccount && view === "accounts" && <div
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
      </div>}

      {!native && !error && <div className="error-banner" role="status"><CircleAlert size={16} /><span>Browser preview only. Account actions need the Windows tray app.</span></div>}
      {error && <div className="error-banner" role="alert"><CircleAlert size={16} /><span>{error}</span><button aria-label="Dismiss error" onClick={() => setError(null)}><X size={14} /></button></div>}
      <footer className="footer"><span className="footer-status"><span className={footerLoggedIn ? "status-dot good" : "status-dot idle"} /> {footerText}</span><span className="footer-mode">{data.useDeceive ? "DECEIVE" : "RIOT CLIENT"}</span></footer>
    </div>
  );
}

export default App;
