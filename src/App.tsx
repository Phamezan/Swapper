import { useEffect, useRef, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open as browseForFile } from "@tauri-apps/plugin-dialog";
import {
  disable as disableAutostart, enable as enableAutostart, isEnabled as isAutostartEnabled,
} from "@tauri-apps/plugin-autostart";
import {
  ArrowLeft, ArrowRight, Check, ChevronRight, CircleAlert, LoaderCircle,
  Pencil, Plus, RefreshCw, Settings2, Trash2, Wrench, X,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { BrandIcon } from "@/components/BrandIcon";
import { RepairPanel } from "./accounts/RepairPanel";
import { CreateShortcutButton } from "./accounts/CreateShortcutButton";
import { RunesPanel } from "./runes/RunesPanel";
import { PairingNotice } from "./settings/PairedDevices";
import { UpdateBanner } from "./settings/UpdateSection";
import type { RunesView, ProBuildsView, KeystoneBuildView, Selection } from "./runes/types";
import { SettingsView } from "./settings/SettingsView";
import type { SettingsTab } from "./settings/SettingsView";
import "./App.css";

export type Account = {
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
export type DetectOutcome =
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
export type RemoteStatus = {
  enabled: boolean;
  state: RemoteState;
  address: string | null;
  tailscaleAddress: string | null;
  lanAddress: string | null;
  lanMessage: string | null;
  message: string | null;
  tailscaleInstalled: boolean;
  tailscaleRunning: boolean;
  dnsName: string | null;
  localAddress: string | null;
  leagueRunning: boolean;
  lcuConnected: boolean;
};
export type AppState = {
  accounts: Account[];
  activeId: string | null;
  isSwitching?: boolean;
  useDeceive: boolean;
  riotExe: string | null;
  riotDetected: boolean;
  deceiveDetected: boolean;
  autoApplyTopPreset: boolean;
  runeTier: string;
  applySpellsWithRunes: boolean;
  hotkey: string | null;
  hotkeyActive: boolean;
  notificationsEnabled: boolean;
  readyCheckNotifications: boolean;
  remote: RemoteStatus;
};
type ChampSelectStatus = {
  phase: string;
  championId: number;
  championName: string;
  position: string;
  locked: boolean;
};
type View = "accounts" | "add" | "edit" | "remove" | "repair" | "settings" | "runes";

const emptyRemote: RemoteStatus = {
  enabled: false, state: "disabled", address: null, tailscaleAddress: null,
  lanAddress: null, lanMessage: null, message: null,
  tailscaleInstalled: false, tailscaleRunning: false, dnsName: null, localAddress: null,
  leagueRunning: false, lcuConnected: false,
};

const empty: AppState = {
  accounts: [], activeId: null, isSwitching: false, useDeceive: false,
  riotExe: null, riotDetected: false, deceiveDetected: false,
  autoApplyTopPreset: false, runeTier: "emerald_plus", applySpellsWithRunes: true,
  hotkey: null, hotkeyActive: false,
  notificationsEnabled: true, readyCheckNotifications: true, remote: emptyRemote,
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
  const [switchingAccountId, setSwitchingAccountId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [editing, setEditing] = useState<string | null>(null);
  const [pendingRemove, setPendingRemove] = useState<string | null>(null);
  // Switch failure that left a dead saved session, and the account currently
  // going through the repair flow.
  const [repairableId, setRepairableId] = useState<string | null>(null);
  const [repairingId, setRepairingId] = useState<string | null>(null);
  const [accountMenu, setAccountMenu] = useState<{ id: string; x: number; y: number } | null>(null);
  const [useDeceive, setUseDeceive] = useState(false);
  const [riotExe, setRiotExe] = useState("");
  const [addStage, setAddStage] = useState<"signIn" | "identify">("signIn");
  const [detect, setDetect] = useState<DetectState>({ phase: "idle" });
  const [detectRound, setDetectRound] = useState(0);
  const [detectedPrompt, setDetectedPrompt] = useState<DetectedIdentity | null>(null);
  const detectionEpoch = useRef(0);
  const previousActiveId = useRef<string | null>(null);
  const [remoteTransport, setRemoteTransport] = useState<"lan" | "tailscale">("lan");
  const [settingsTab, setSettingsTab] = useState<SettingsTab>("general");
  const [startOnStartup, setStartOnStartup] = useState(false);
  const [pathMode, setPathMode] = useState<"auto" | "manual" | null>(null);
  const [runes, setRunes] = useState<RunesView | null>(null);
  const [runesLoading, setRunesLoading] = useState(false);
  const [runesBusy, setRunesBusy] = useState(false);
  const [runesError, setRunesError] = useState<string | null>(null);
  const runePosition = useRef<string | undefined>(undefined);
  const runeChampion = useRef(0);
  const runeRequest = useRef(0);
  const native = isTauri();

  useEffect(() => {
    if (!native) return;
    invoke<AppState>("get_state").then(sync).catch(showError);
    const unlisteners = Promise.all([
      listen<string>("navigate", ({ payload }) => {
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
        invoke<ChampSelectStatus>("champ_select_status").then(handleChampSelect).catch(() => {});
      }),
      listen<ChampSelectStatus>("champ_select", ({ payload }) => {
        handleChampSelect(payload);
      }),
      listen("runes_changed", () => {
        void loadRunes();
      }),
      listen<{ id: string; name: string }>("switch_started", ({ payload }) => {
        setSwitchingAccountId(payload.id);
        setRepairableId(null);
        setError(null);
      }),
      listen<{ id: string; view: AppState }>("switch_done", ({ payload }) => {
        setSwitchingAccountId(null);
        sync(payload.view);
      }),
      listen<{ id: string; error: string; repairable: boolean }>("switch_failed", ({ payload }) => {
        setSwitchingAccountId(null);
        setData((prev) => ({ ...prev, activeId: previousActiveId.current }));
        showError(payload.error);
        setRepairableId(payload.repairable ? payload.id : null);
        invoke<AppState>("get_state").then(sync).catch(() => {});
      }),
      listen<{ id: string; name: string; mismatch: boolean }>("session_expired", ({ payload }) => {
        // A background watch decided the switched-in session was dead on
        // arrival; offer the same Repair Account action as a failed switch.
        setRepairableId(payload.id);
        showError(payload.mismatch
          ? `Riot Client signed in to a different account than ${payload.name}. Use Repair Account to fix it.`
          : `The saved session for ${payload.name} has expired. Use Repair Account to sign in again.`);
      }),
    ]);
    return () => {
      unlisteners.then((fns) => fns.forEach((unlisten) => unlisten()));
    };
  }, [native]);

  function sync(next: AppState) {
    setData(next);
    if (!next.isSwitching) {
      setSwitchingAccountId(null);
    }
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

  // The startup toggle reads the real OS autostart entry, not a saved flag.
  useEffect(() => {
    if (!native || view !== "settings") return;
    let live = true;
    isAutostartEnabled()
      .then((enabled) => { if (live) setStartOnStartup(enabled); })
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

  async function loadRunes(position?: string) {
    if (position) runePosition.current = position;
    const selectedPosition = position ?? runePosition.current;
    const request = ++runeRequest.current;
    setRunesLoading(true);
    try {
      const next = await invoke<RunesView>("get_runes", selectedPosition ? { position: selectedPosition } : {});
      if (request === runeRequest.current) {
        setRunes(next);
        runeChampion.current = next.championId;
        setRunesError(null);
      }
    } catch (reason) {
      if (request === runeRequest.current) setRunesError(String(reason));
    } finally {
      if (request === runeRequest.current) setRunesLoading(false);
    }
  }

  async function importItemBuild(
    championId: number,
    championName: string,
    source: string,
    items: number[],
  ) {
    setRunesBusy(true);
    setRunesError(null);
    try {
      await invoke("import_item_build", { championId, championName, source, items });
    } catch (reason) {
      setRunesError(String(reason));
      throw reason;
    } finally {
      setRunesBusy(false);
    }
  }

  async function importKeystoneItemBuild(
    championId: number,
    championName: string,
    source: string,
    build: KeystoneBuildView,
  ) {
    setRunesBusy(true);
    setRunesError(null);
    try {
      await invoke("import_keystone_item_build", { championId, championName, source, build });
    } catch (reason) {
      setRunesError(String(reason));
      throw reason;
    } finally {
      setRunesBusy(false);
    }
  }

  async function applyRunes(selection: Selection, presetIndex: number | null, spells: number[] | null) {
    setRunesBusy(true);
    setRunesError(null);
    try {
      await invoke("apply_rune_page", { selection, presetIndex, spells });
      await loadRunes();
    } catch (reason) {
      setRunesError(String(reason));
    } finally {
      setRunesBusy(false);
    }
  }

  async function pickSpell(slot: "d" | "f", spellId: number) {
    setRunesBusy(true);
    setRunesError(null);
    try {
      await invoke("apply_spell", { slot, spellId });
      await loadRunes();
    } catch (reason) {
      setRunesError(String(reason));
    } finally {
      setRunesBusy(false);
    }
  }

  async function setApplySpellsWithRunes(enabled: boolean) {
    setRunesBusy(true);
    setRunesError(null);
    try {
      const next = await invoke<AppState>("set_apply_spells_with_runes", { enabled });
      sync(next);
      await loadRunes();
    } catch (reason) {
      showError(reason);
    } finally {
      setRunesBusy(false);
    }
  }

  function loadProBuilds(championId: number, position: string, page: number): Promise<ProBuildsView> {
    return invoke<ProBuildsView>("get_pro_builds", { championId, position, page });
  }

  function loadKeystoneBuild(
    championId: number,
    position: string,
    tier: string,
    keystone: number,
  ): Promise<KeystoneBuildView | null> {
    return invoke<KeystoneBuildView | null>("get_keystone_build", {
      championId,
      position,
      tier,
      keystone,
    });
  }

  async function setAutoApply(enabled: boolean) {
    setRunesBusy(true);
    setRunesError(null);
    try {
      const next = await invoke<AppState>("set_auto_apply_top_preset", { enabled });
      sync(next);
      await loadRunes();
    } catch (reason) {
      showError(reason);
    } finally {
      setRunesBusy(false);
    }
  }

  async function setRuneTier(tier: string) {
    setRunesBusy(true);
    setRunesError(null);
    try {
      const next = await invoke<AppState>("set_rune_tier", { tier });
      sync(next);
      await loadRunes();
    } catch (reason) {
      setRunesError(String(reason));
    } finally {
      setRunesBusy(false);
    }
  }

  const setNotifications = (enabled: boolean) =>
    action("notifications", () => invoke<AppState>("set_notifications_enabled", { enabled }));
  const setReadyCheckNotifications = (enabled: boolean) =>
    action("ready-check-notifications", () => invoke<AppState>("set_ready_check_notifications", { enabled }));

  function handleChampSelect(payload: ChampSelectStatus) {
    if (payload.phase === "ChampSelect") {
      if (runeChampion.current !== payload.championId) runePosition.current = undefined;
      setView("runes");
      void loadRunes();
    } else {
      setView((current) => (current === "runes" ? "accounts" : current));
    }
  }

  async function setRemote(enabled: boolean) {
    if (!native) {
      setError("Remote Control is available in the Windows tray app. Start it with npm run tauri dev.");
      return;
    }
    const previous = data.remote;
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

  async function setStartOnStartupSetting(enabled: boolean) {
    await action("autostart", async () => {
      if (enabled) await enableAutostart();
      else await disableAutostart();
      setStartOnStartup(await isAutostartEnabled());
    });
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
    setView(next);
    setAccountMenu(null);
    setError(null);
    setEditing(null);
    setPendingRemove(null);
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
  const isSwitching = Boolean(switchingAccountId || data.isSwitching);
  const isBusy = busy !== null || isSwitching;

  const openAccountMenu = (id: string, x: number, y: number) => {
    if (isBusy) return;
    setAccountMenu({
      id,
      x: Math.max(8, Math.min(x, window.innerWidth - 192)),
      y: Math.max(8, Math.min(y, window.innerHeight - 164)),
    });
  };
  const menuAccount = data.accounts.find((account) => account.id === accountMenu?.id);
  const switchToAccount = async (id: string) => {
    if (!native) {
      setError("Account actions are available in the Windows tray app. Start it with npm run tauri dev.");
      return;
    }
    if (isSwitching) return;
    setError(null);
    setAccountMenu(null);
    detectionEpoch.current += 1;
    setDetectedPrompt(null);
    previousActiveId.current = data.activeId;
    setSwitchingAccountId(id);
    setData((prev) => ({ ...prev, activeId: id }));
    try {
      await invoke("switch_account", { id });
    } catch (reason) {
      setSwitchingAccountId(null);
      setData((prev) => ({ ...prev, activeId: previousActiveId.current }));
      showError(reason);
    }
  };
  const active = data.accounts.find((account) => account.id === data.activeId);
  // Opens Riot Client for the affected account, then hands over to the
  // repair flow, which waits for that account's sign-in.
  const startRepair = (id: string) => {
    if (!data.accounts.some((account) => account.id === id)) {
      setRepairableId(null);
      return;
    }
    void action("repair-begin", () => invoke<void>("begin_repair", { id }), () => {
      setRepairableId(null);
      setRepairingId(id);
      navigate("repair");
    });
  };
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

  const changeRiotPathMode = (mode: "auto" | "manual") => {
    if (mode === "manual") {
      setPathMode("manual");
      return;
    }
    setPathMode("auto");
    setRiotExe("");
    void applySettings({ useDeceive, riotExe: null });
  };

  const commitRiotPath = (value: string | null) => {
    if (value === null) {
      setPathMode("auto");
      void applySettings({ useDeceive, riotExe: null });
      return;
    }
    void applySettings({ useDeceive, riotExe: value });
  };

  // The backend registers the combination before persisting it, so a rejection
  // (invalid or already owned by another app) leaves the previous one working.
  // HotkeySetting shows the rejection inline instead of the shared error banner.
  const applyHotkey = async (value: string | null) => {
    if (!native) {
      setError("The global hotkey is available in the Windows tray app. Start it with npm run tauri dev.");
      return;
    }
    sync(await invoke<AppState>("set_hotkey", { shortcut: value }));
  };

  const browseRiotPath = async () => {
    if (!native) return;
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
    }
  };

  return (
    <div className="shell dark">
      <header className="topbar">
        <BrandIcon className="brand-mark" size={35} />
        <div className="brand-copy"><strong>Swapper</strong><span>RIOT ACCOUNTS</span></div>
        <button className="icon-button close-button" aria-label="Hide Swapper" onClick={() => { if (native) void invoke("hide_flyout"); }}> <X size={16} /> </button>
      </header>

      <main className="main-panel">
        {view === "accounts" ? (
          <>
            <div className="panel-heading">
              <div><p className="eyebrow">{data.accounts.length === 0 ? "GET STARTED" : "READY TO PLAY"}</p><h1>Your accounts</h1></div>
              <button className="icon-button" aria-label="Settings" disabled={isBusy} onClick={() => navigate("settings")}><Settings2 size={19} /></button>
            </div>
            <UpdateBanner onOpenSettings={() => navigate("settings")} />
            {visibleDetectedPrompt && (
              <section className="detected-prompt">
                <strong className="detected-title">Signed-in account detected</strong>
                <IdentityCard identity={visibleDetectedPrompt} />
                <Button className="primary-action" disabled={isBusy} onClick={() => saveDetected(visibleDetectedPrompt, "save-detected", () => setDetectedPrompt(null))}>
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
                            ? "Current session"
                            : repairableId === account.id
                              ? "Session needs repair"
                              : "Click to switch"}
                        {account.region ? ` · ${account.region}` : ""}
                      </small>
                    </span>
                    {isThisSwitching || busy === `switch-${account.id}` ? (
                      <LoaderCircle className="spin" size={20} />
                    ) : isActive ? (
                      <span className="active-indicator"><Check size={14} /></span>
                    ) : (
                      <ChevronRight size={20} className="row-chevron" />
                    )}
                  </button>
                );
              })}
            </div>
            {data.accounts.length > 0 && <Button className="add-inline" disabled={isBusy} onClick={() => navigate("add")}><Plus size={17} /> Add account</Button>}
          </>
        ) : view === "runes" ? (
          <RunesPanel
            mode="desktop"
            view={runes}
            loading={runesLoading}
            busy={runesBusy}
            error={runesError}
            onApply={(selection, presetIndex, spells) => void applyRunes(selection, presetIndex, spells)}
            onToggleAutoApply={(enabled) => void setAutoApply(enabled)}
            onToggleSpellsWithRunes={(enabled) => void setApplySpellsWithRunes(enabled)}
            onPickSpell={(slot, spellId) => void pickSpell(slot, spellId)}
            onPositionChange={(position) => void loadRunes(position)}
            onImportItems={importItemBuild}
            onImportPresetBuild={importKeystoneItemBuild}
            onLoadProBuilds={loadProBuilds}
            onLoadBuild={loadKeystoneBuild}
            onTierChange={(tier) => void setRuneTier(tier)}
          />
        ) : (
          <>
            <div className="panel-heading compact">
              <button className="icon-button back-button" aria-label="Back to accounts" onClick={() => navigate("accounts")}><ArrowLeft size={17} /></button>
              <div><p className="eyebrow">SWAPPER</p><h1>{view === "add" ? "Add account" : view === "settings" ? "Settings" : view === "edit" ? "Edit accounts" : view === "repair" ? "Repair account" : "Remove account"}</h1></div>
            </div>

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
                <p>Use the official Riot Client and turn on “Stay signed in”. Swapper never asks for your password. League does not need to be open.</p>
                <Button className="primary-action" disabled={busy !== null} onClick={() => action("launch", async () => {
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
                {view === "edit" && editing !== account.id && (
                  <CreateShortcutButton accountId={account.id} accountName={account.name} disabled={busy !== null} onError={showError} />
                )}
                {view === "edit" ? editing === account.id
                  ? <button className="icon-button" aria-label={`Save nickname for ${account.name}`} onClick={() => action("nickname", () => invoke<AppState>("set_nickname", { id: account.id, name }), () => setEditing(null))}><Check size={17} /></button>
                  : <button className="icon-button" aria-label={`Set a nickname for ${account.name}`} onClick={() => { setEditing(account.id); setName(account.nickname ?? ""); }}><Pencil size={16} /></button>
                  : <button className="icon-button danger-icon" aria-label={`Remove ${account.name}`} onClick={() => setPendingRemove(account.id)}><Trash2 size={16} /></button>}
                {pendingRemove === account.id && view === "remove" && <div className="remove-confirm"><span>Remove this saved session?</span><Button size="sm" variant="destructive" disabled={busy !== null} onClick={() => action("remove", () => invoke<AppState>("remove_account", { id: account.id }), () => setPendingRemove(null))}>Remove</Button><Button size="sm" variant="ghost" onClick={() => setPendingRemove(null)}>Cancel</Button></div>}
              </div>)}
            </div>}

            {view === "settings" && <SettingsView
              state={data}
              busy={busy !== null}
              tab={settingsTab}
              onTabChange={setSettingsTab}
              useDeceive={useDeceive}
              onUseDeceiveChange={(checked) => void applySettings({ useDeceive: checked, riotExe: configuredPath() })}
              startOnStartup={startOnStartup}
              onStartOnStartupChange={(enabled) => void setStartOnStartupSetting(enabled)}
              manualPath={manualPath}
              riotExe={riotExe}
              onRiotExeChange={setRiotExe}
              onPathModeChange={changeRiotPathMode}
              onRiotPathCommit={commitRiotPath}
              onBrowseRiotPath={() => void browseRiotPath()}
              onSetHotkey={applyHotkey}
              onNotificationsChange={(enabled) => void setNotifications(enabled)}
              onReadyCheckChange={(enabled) => void setReadyCheckNotifications(enabled)}
              onAutoApplyChange={(enabled) => void setAutoApply(enabled)}
              onApplySpellsChange={(enabled) => void setApplySpellsWithRunes(enabled)}
              onRuneTierChange={(tier) => void setRuneTier(tier)}
              remoteTransport={remoteTransport}
              onRemoteTransportChange={setRemoteTransport}
              onRemoteToggle={(enabled) => void setRemote(enabled)}
              onError={showError}
            />}
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
      <PairingNotice />
      {error && <div className="error-banner" role="alert"><CircleAlert size={16} /><span>{error}</span>{repairableId && <Button size="sm" variant="outline" className="repair-banner-action" onClick={() => startRepair(repairableId)}><Wrench size={14} /> Repair Account</Button>}<button aria-label="Dismiss error" onClick={() => { setRepairableId(null); setError(null); }}><X size={14} /></button></div>}
      <footer className="footer"><span className="footer-status"><span className={footerLoggedIn ? "status-dot good" : "status-dot idle"} /> {footerText}</span><span className="footer-mode">{data.useDeceive ? "DECEIVE" : "RIOT CLIENT"}</span></footer>
    </div>
  );
}

export default App;
