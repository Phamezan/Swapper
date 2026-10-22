import { useEffect, useRef, useState, type ComponentType, type ComponentProps } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open as browseForFile } from "@tauri-apps/plugin-dialog";
import {
  disable as disableAutostart, enable as enableAutostart, isEnabled as isAutostartEnabled,
} from "@tauri-apps/plugin-autostart";
import {
  ArrowRight, ChevronRight, LoaderCircle,
  Pencil, Trash2, Wrench, X,
} from "lucide-react";
import {
  AddAccountIcon, AddIcon, BackIcon, CheckIcon, ErrorIcon, InfoIcon, RefreshIcon, SettingsIcon, WarningIcon,
} from "./full-app/ui/icons";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { BrandIcon } from "@/components/BrandIcon";
import { RepairPanel } from "./accounts/RepairPanel";
import { CreateShortcutButton } from "./accounts/CreateShortcutButton";
import { RunesPanel } from "./runes/RunesPanel";
import { ChampionTab } from "./runes/ChampionTab";
import { PairingNotice } from "./settings/PairedDevices";
import { UpdateBanner } from "./settings/UpdateSection";
import type { RunesView, ProBuildsView, PresetBuildView, MatchupView, ChampionApi, Selection, ItemsImported, ShopNotice } from "./runes/types";
import { isGamePhase, shopNoticeFrom } from "./runes/types";
import { SettingsView } from "./settings/SettingsView";
import { LiveGameCompact } from "./full-app/LiveGameCompact";
import { PageHeader } from "./full-app/ui/PageHeader";
import { RiotClientIcon } from "./riotIcon";
import type { SettingsTab } from "./settings/SettingsView";
import type { AccountCardsProps } from "./full-app/AccountCards";
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
  lanFirewallNeeded: boolean;
  message: string | null;
  tailscaleInstalled: boolean;
  tailscaleRunning: boolean;
  dnsName: string | null;
  localAddress: string | null;
  leagueRunning: boolean;
  lcuConnected: boolean;
  lanReconnectNeeded: boolean;
  lanReconnectOldAddress: string | null;
  lanReconnectNewAddress: string | null;
  lanReconnectSince: number | null;
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
  importItemsWithRunes: boolean;
  hotkey: string | null;
  hotkeyActive: boolean;
  notificationsEnabled: boolean;
  readyCheckNotifications: boolean;
  koreanFont: boolean;
  ltkInstalled: boolean;
  remote: RemoteStatus;
};
type ChampSelectStatus = {
  phase: string;
  championId: number;
  championName: string;
  position: string;
  locked: boolean;
};
export type View = "accounts" | "add" | "edit" | "remove" | "repair" | "settings" | "runes" | "champions";
/** Lets the Fullsized App host this component as its Accounts, Champion select
 *  and Settings pages. The flyout passes nothing. */
export type FullAppBridge = {
  Accounts?: ComponentType<AccountCardsProps>;
  Runes?: ComponentType<ComponentProps<typeof RunesPanel>>;
  Settings?: ComponentType<ComponentProps<typeof SettingsView>>;
  /** A new object navigates once, e.g. from the sidebar. */
  request: { view: View; switchId?: string } | null;
  /** Riot ID League Client is signed in with (null when unknown), so the status
   *  bar agrees with the Dashboard even when the login did not go through Swapper. */
  signedIn?: string | null;
  onViewChange: (view: View) => void;
  onChampSelectChange: (active: boolean) => void;
};

const emptyRemote: RemoteStatus = {
  enabled: false, state: "disabled", address: null, tailscaleAddress: null,
  lanAddress: null, lanMessage: null, lanFirewallNeeded: false, message: null,
  tailscaleInstalled: false, tailscaleRunning: false, dnsName: null, localAddress: null,
  leagueRunning: false, lcuConnected: false,
  lanReconnectNeeded: false, lanReconnectOldAddress: null, lanReconnectNewAddress: null,
  lanReconnectSince: null,
};

const empty: AppState = {
  accounts: [], activeId: null, isSwitching: false, useDeceive: false,
  riotExe: null, riotDetected: false, deceiveDetected: false,
  autoApplyTopPreset: false, runeTier: "emerald_plus", applySpellsWithRunes: true, importItemsWithRunes: true,
  hotkey: null, hotkeyActive: false,
  notificationsEnabled: true, readyCheckNotifications: true, koreanFont: false, ltkInstalled: false, remote: emptyRemote,
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

function App({ fullApp }: { fullApp?: FullAppBridge } = {}) {
  const RunesSurface = fullApp?.Runes ?? RunesPanel;
  const SettingsSurface = fullApp?.Settings ?? SettingsView;
  const AccountsSurface = fullApp?.Accounts;
  const [data, setData] = useState<AppState>(empty);
  const [view, setView] = useState<View>("accounts");
  const [inChampSelect, setInChampSelect] = useState(false);
  const [modeSpin, setModeSpin] = useState(0);
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
  const [koreanFont, setKoreanFont] = useState(false);
  const [ltkInstalling, setLtkInstalling] = useState(false);
  const [ltkInstallError, setLtkInstallError] = useState<string | null>(null);
  const [riotExe, setRiotExe] = useState("");
  const [addStage, setAddStage] = useState<"signIn" | "identify">("signIn");
  const [detect, setDetect] = useState<DetectState>({ phase: "idle" });
  const [detectRound, setDetectRound] = useState(0);
  const [detectedPrompt, setDetectedPrompt] = useState<DetectedIdentity | null>(null);
  const detectionEpoch = useRef(0);
  const previousActiveId = useRef<string | null>(null);
  const [remoteTransport, setRemoteTransport] = useState<"lan" | "tailscale">("lan");
  const [settingsTab, setSettingsTab] = useState<SettingsTab>("general");
  // Bumped to ask the Remote tab to open its QR panel, e.g. from the reconnect
  // banner's Show QR button.
  const [remoteQrRequest, setRemoteQrRequest] = useState(0);
  const [startOnStartup, setStartOnStartup] = useState(false);
  const [pathMode, setPathMode] = useState<"auto" | "manual" | null>(null);
  const [runes, setRunes] = useState<RunesView | null>(null);
  const [runesLoading, setRunesLoading] = useState(false);
  const [runesBusy, setRunesBusy] = useState(false);
  const [runesError, setRunesError] = useState<string | null>(null);
  const [shopNotice, setShopNotice] = useState<ShopNotice | null>(null);
  const runePosition = useRef<string | undefined>(undefined);
  const runeChampion = useRef(0);
  const runeRequest = useRef(0);
  // Bumped when a game ends, so the Champion tab drops its search instead of
  // keeping it for the next champion select.
  const [runeSession, setRuneSession] = useState(0);
  const lastPhase = useRef("");
  const native = isTauri();
  const onChampSelectChange = useRef(fullApp?.onChampSelectChange);
  onChampSelectChange.current = fullApp?.onChampSelectChange;

  useEffect(() => {
    if (!native) return;
    invoke<AppState>("get_state").then(sync).catch(showError);
    const unlisteners = Promise.all([
      listen<string>("navigate", ({ payload }) => {
        if (["accounts", "add", "edit", "remove", "settings"].includes(payload)) {
          // Opening Swapper during champion select lands on the champ select page.
          setView(payload === "accounts" && lastPhase.current === "ChampSelect" ? "runes" : payload as View);
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
      // Background shop imports have no confirmed result on the apply
      // command, so this event reports the shop status separately.
      listen<ItemsImported>("items_imported", ({ payload }) => setShopNotice(shopNoticeFrom(payload))),
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
          ? `Riot Client signed in to a different account than ${payload.name}. Use Repair account to fix it.`
          : `The saved session for ${payload.name} has expired. Use Repair account to sign in again.`);
      }),
      listen("lan_reconnect_changed", () => {
        invoke<AppState>("get_state").then(sync).catch(() => {});
      }),
    ]);
    // Listeners exist now; a newly created window is shown (and sent its
    // destination) only after this.
    void unlisteners.then(() => invoke("window_ready")).catch(() => {});
    return () => {
      unlisteners.then((fns) => fns.forEach((unlisten) => unlisten()));
    };
  }, [native]);

  useEffect(() => { fullApp?.onViewChange(view); }, [view]);
  useEffect(() => {
    if (!fullApp?.request) return;
    navigate(fullApp.request.view);
    if (fullApp.request.view === "runes") void loadRunes();
    // The Dashboard's Switch buttons reuse the Accounts page's switch flow.
    if (fullApp.request.switchId) void switchToAccount(fullApp.request.switchId);
  }, [fullApp?.request]);

  function sync(next: AppState) {
    setData(next);
    if (!next.isSwitching) {
      setSwitchingAccountId(null);
    }
    setUseDeceive(next.useDeceive);
    setRiotExe(next.riotExe ?? "");
    setKoreanFont(next.koreanFont);
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
        if (next.championId !== runeChampion.current) setShopNotice(null);
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
    items: number[],
  ) {
    setRunesBusy(true);
    setRunesError(null);
    setShopNotice(null);
    try {
      await invoke("import_item_build", { championId, championName, items });
    } catch (reason) {
      setRunesError(String(reason));
      throw reason;
    } finally {
      setRunesBusy(false);
    }
  }

  async function applyRunes(
    selection: Selection,
    presetIndex: number | null,
    spells: number[] | null,
    enemyChampionId?: number | null,
    highestWin = false,
  ) {
    setRunesBusy(true);
    setRunesError(null);
    setShopNotice(null);
    try {
      await invoke("apply_rune_page", {
        selection,
        presetIndex,
        spells,
        position: runes?.position ?? null,
        enemyChampionId: enemyChampionId ?? null,
        highestWin,
      });
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

  // Only on this click does Windows ask for firewall permission, for Swapper.
  async function allowLanFirewall() {
    try {
      sync(await invoke<AppState>("allow_lan_firewall"));
      // The service restarts in the background; pick up the LAN address.
      window.setTimeout(() => { invoke<AppState>("get_state").then(sync).catch(() => {}); }, 1500);
    } catch (reason) {
      showError(reason);
    }
  }

  async function setImportItemsWithRunes(enabled: boolean) {
    setRunesBusy(true);
    setRunesError(null);
    try {
      const next = await invoke<AppState>("set_import_items_with_runes", { enabled });
      sync(next);
      await loadRunes();
    } catch (reason) {
      showError(reason);
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

  function loadProBuilds(
    championId: number,
    position: string,
    page: number,
    otp: boolean,
  ): Promise<ProBuildsView> {
    return invoke<ProBuildsView>("get_pro_builds", { championId, position, page, isOtp: otp });
  }

  function loadKeystoneBuild(
    championId: number,
    position: string,
    tier: string,
    keystone: number,
    mostPlayed?: Selection,
  ): Promise<PresetBuildView | null> {
    return invoke<PresetBuildView | null>("get_keystone_build", {
      championId,
      position,
      tier,
      keystone,
      mostPlayed: mostPlayed ?? null,
    });
  }

  function loadMatchup(
    championId: number,
    enemyChampionId: number,
    position: string,
    tier: string,
  ): Promise<MatchupView | null> {
    return invoke<MatchupView | null>("get_matchup", { championId, enemyChampionId, position, tier });
  }

  const championApi: ChampionApi = {
    list: () => invoke("get_champion_list"),
    counters: (championId, position, tier) =>
      invoke("get_champion_counters", { championId, position, tier }),
    overview: (championId, position, tier) =>
      invoke("get_champion_overview", { championId, position, tier }),
    tierList: (position, tier) => invoke("get_tier_list", { position, tier }),
    duoList: (tier) => invoke("get_duo_list", { tier }),
  };

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
  const setKoreanFontSetting = (enabled: boolean) =>
    action("korean-font", () => invoke<AppState>("set_korean_font", { enabled }));

  async function installLtkManager() {
    if (!native) {
      setError("Installing LTK Manager is available in the Windows tray app. Start it with npm run tauri dev.");
      return;
    }
    setLtkInstalling(true);
    setLtkInstallError(null);
    try {
      await invoke("install_ltk_manager");
      sync(await invoke<AppState>("get_state"));
    } catch (reason) {
      setLtkInstallError(String(reason));
    } finally {
      setLtkInstalling(false);
    }
  }

  function handleChampSelect(payload: ChampSelectStatus) {
    // Leaving a live game bumps the rune session, so the Champion tab's search
    // is cleared before the next champion select opens.
    const inGame = isGamePhase(payload.phase);
    if (isGamePhase(lastPhase.current) && !inGame) setRuneSession((id) => id + 1);
    setInChampSelect(payload.phase === "ChampSelect");
    if ((payload.phase === "ChampSelect") !== (lastPhase.current === "ChampSelect")) {
      onChampSelectChange.current?.(payload.phase === "ChampSelect");
    }
    if (payload.phase === "ChampSelect") {
      if (runeChampion.current !== payload.championId) runePosition.current = undefined;
      if (lastPhase.current !== "ChampSelect") setView("runes");
      void loadRunes();
    } else {
      setView((current) => (current === "runes" ? "accounts" : current));
    }
    lastPhase.current = payload.phase;
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

  // Jumps to Settings → Remote and asks the QR panel to open, for re-pairing a
  // phone after the PC's LAN address changed.
  const showRemoteQr = () => {
    setRemoteTransport("lan");
    setSettingsTab("remote");
    setRemoteQrRequest((request) => request + 1);
    navigate("settings");
  };

  const dismissReconnect = () => {
    if (!native) return;
    invoke("dismiss_lan_reconnect")
      .then(() => invoke<AppState>("get_state").then(sync))
      .catch(showError);
  };

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
  const pageTitle = view === "add" ? "Add account" : view === "settings" ? "Settings" : view === "edit" ? "Edit accounts" : view === "repair" ? "Repair account" : "Remove account";
  const footerUnsaved = Boolean(detectedPrompt && !matchingDetectedAccount);
  // The saved account Swapper switched to, else whoever League Client reports
  // (a login made outside Swapper is still a login).
  const clientName = fullApp?.signedIn ? data.accounts.find((account) => account.riotId?.toLowerCase() === fullApp.signedIn!.toLowerCase())?.name ?? fullApp.signedIn.split("#")[0] : null;
  const footerText = footerUnsaved
    ? "Unsaved account detected"
    : active
      ? `Logged in as ${active.name}`
      : clientName
        ? `Signed in as ${clientName}`
        : "Not logged in";
  const footerLoggedIn = !footerUnsaved && Boolean(active || clientName);
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

  const toolsOpen = view === "champions" || view === "runes";
  const brandMode = toolsOpen ? "champions" : "accounts";
  const otherMode = toolsOpen ? "accounts" : inChampSelect ? "runes" : "champions";
  const switchLabel = toolsOpen ? "Switch to accounts" : "Switch to champion lookup";

  return (
    <div className={`shell dark${fullApp ? " full-app-embedded" : ""}`}>
      {!fullApp && <header className="topbar">
        <button type="button" className={`brand-button mode-${brandMode}`} aria-label={switchLabel} title={switchLabel} onClick={() => {
          setModeSpin((spin) => spin + 180);
          navigate(otherMode);
        }}>
          <span className="brand-icon" style={{ transform: `rotate(${modeSpin}deg)` }}>
            <span style={{ transform: `rotate(${-modeSpin}deg)` }}>
              {brandMode === "accounts" ? <BrandIcon className="brand-mark" size={22} /> : (
                <svg width="19" height="19" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
                  <path fillRule="evenodd" d="M12 1.8C6.6 1.8 3.4 5.8 3.4 10.6v5L7.2 20.4v2.2l3-2V22h3.6v-1.4l3 2v-2.2l3.8-4.8v-5C20.6 5.8 17.4 1.8 12 1.8zM6 9.8h5V19l-2.2 1.4v-8.2H6zM18 9.8h-5V19l2.2 1.4v-8.2H18z" />
                </svg>
              )}
            </span>
          </span>
          <span className="brand-copy">
            <strong>{brandMode === "accounts" ? "Swapper" : "Champions"}{toolsOpen && inChampSelect && <i className="brand-live-dot" aria-label="Live champion select" />}</strong>
            <span>⇄ <span className={`mode-${toolsOpen ? "accounts" : "champions"}`}>{toolsOpen ? "ACCOUNTS" : "CHAMPIONS"}</span></span>
          </span>
        </button>
        <button className="icon-button close-button" aria-label="Hide Swapper" onClick={() => { if (native) void invoke("hide_flyout"); }}> <X size={16} /> </button>
      </header>}

      <main className={`main-panel${view === "settings" ? " settings-panel" : ""}${toolsOpen ? " runes-panel-host" : ""}`}>
        {view === "accounts" ? (
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
        ) : view === "champions" ? (
          <div className="runes-panel runes-desktop">
            <div className="runes-body">
              <ChampionTab
                mode="desktop"
                initialTier={data.runeTier}
                prefillChampion={null}
                prefillPosition=""
                sessionKey={String(runeSession)}
                api={championApi}
                onLoadMatchup={loadMatchup}
              />
            </div>
          </div>
        ) : view === "runes" ? (
          <RunesSurface
            mode="desktop"
            view={runes}
            loading={runesLoading}
            busy={runesBusy}
            error={runesError}
            shopNotice={shopNotice}
            onApply={applyRunes}
            onToggleAutoApply={(enabled) => void setAutoApply(enabled)}
            onToggleSpellsWithRunes={(enabled) => void setApplySpellsWithRunes(enabled)}
            onToggleImportItems={(enabled) => void setImportItemsWithRunes(enabled)}
            onPickSpell={(slot, spellId) => void pickSpell(slot, spellId)}
            onPositionChange={(position) => void loadRunes(position)}
            onImportItems={importItemBuild}
            onLoadProBuilds={loadProBuilds}
            onLoadBuild={loadKeystoneBuild}
            onLoadMatchup={loadMatchup}
            championApi={championApi}
            onTierChange={(tier) => void setRuneTier(tier)}
            sessionKey={String(runeSession)}
          />
        ) : (
          <>
            {fullApp ? <PageHeader title={pageTitle} crumbs={view === "settings" ? undefined : [{ label: "Accounts", onClick: () => navigate("accounts") }]} /> : <div className="panel-heading compact">
              <button className="icon-button back-button" aria-label="Back to accounts" onClick={() => navigate("accounts")}><BackIcon size={17} /></button>
              <div><p className="eyebrow">Swapper</p><h1>{pageTitle}</h1></div>
            </div>}

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

            {view === "settings" && <SettingsSurface
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
              koreanFont={koreanFont}
              onKoreanFontChange={(enabled) => void setKoreanFontSetting(enabled)}
              ltkInstalled={data.ltkInstalled}
              ltkInstalling={ltkInstalling}
              ltkInstallError={ltkInstallError}
              onInstallLtk={() => void installLtkManager()}
              onAutoApplyChange={(enabled) => void setAutoApply(enabled)}
              onApplySpellsChange={(enabled) => void setApplySpellsWithRunes(enabled)}
              onImportItemsChange={(enabled) => void setImportItemsWithRunes(enabled)}
              onAllowLanFirewall={allowLanFirewall}
              onRuneTierChange={(tier) => void setRuneTier(tier)}
              remoteTransport={remoteTransport}
              onRemoteTransportChange={setRemoteTransport}
              onRemoteToggle={(enabled) => void setRemote(enabled)}
              qrRequest={remoteQrRequest}
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

      {!native && !error && <div className="error-banner" role="status"><InfoIcon size={16} /><span>Browser preview only. Account actions need the Windows tray app.</span></div>}
      <PairingNotice />
      {error && <div className="error-banner" role="alert"><ErrorIcon size={16} /><span>{error}</span>{repairableId && <Button size="sm" variant="outline" className="repair-banner-action" onClick={() => startRepair(repairableId)}><Wrench size={14} /> Repair account</Button>}<button aria-label="Dismiss error" onClick={() => { setRepairableId(null); setError(null); }}><X size={14} /></button></div>}
      {/* The Fullsized App shows status in its sidebar podium instead. */}
      {!fullApp && <footer className="footer"><span className="footer-status"><span className={footerLoggedIn ? "status-dot good" : "status-dot idle"} /> {footerText}</span><span className="footer-mode">{data.useDeceive ? "DECEIVE" : "RIOT CLIENT"}</span></footer>}
    </div>
  );
}

export default App;
