import { useEffect, useRef, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open as browseForFile } from "@tauri-apps/plugin-dialog";
import {
  disable as disableAutostart, enable as enableAutostart, isEnabled as isAutostartEnabled,
} from "@tauri-apps/plugin-autostart";
import type { RunesView, ProBuildsView, PresetBuildView, MatchupView, ChampionApi, Selection, ItemsImported, ShopNotice } from "../runes/types";
import { isGamePhase, shopNoticeFrom } from "../runes/types";
import type { SettingsTab } from "../settings/SettingsView";
import { useSettingsVersion } from "../settingsEvents";
import type { AppState, RemoteStatus, DetectOutcome, DetectState, DetectedIdentity, ChampSelectStatus, View, FullAppBridge } from "./types";
import type { DeceivePrompt } from "../settings/DeceiveRestartNotice";
import { riotIdOf } from "./IdentityCard";

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

export function useAppController(fullApp?: FullAppBridge) {
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
  const [deceivePrompt, setDeceivePrompt] = useState<DeceivePrompt | null>(null);
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
  const settingsVersion = useSettingsVersion();
  // Settings changed elsewhere (first-run setup): reload what this window shows.
  useEffect(() => {
    if (native && settingsVersion > 0) invoke<AppState>("get_state").then(sync).catch(() => {});
  }, [settingsVersion]);

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
  }, [native, view, settingsVersion]);

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

  // Deceive only takes effect on the next client launch, so a running client is restarted
  // through the normal switch onto the signed-in account (the backend decides when that is safe).
  const changeDeceive = async (checked: boolean) => {
    setDeceivePrompt(null);
    let saved = false;
    await action("settings", async () => {
      await invoke("save_settings", { useDeceive: checked, riotExe: configuredPath() });
      saved = true;
    });
    if (!native) return;
    void invoke<AppState>("get_state").then(sync).catch(showError);
    if (!saved) return;
    const plan = await invoke<string>("restart_plan").catch(() => "save");
    // Without a saved session Swapper can't sign the client back in, so the user restarts it.
    if (!data.activeId) { if (plan !== "save") setDeceivePrompt("unsaved"); return; }
    if (plan === "restart") void switchToAccount(data.activeId);
    else if (plan === "ask" || plan === "defer") setDeceivePrompt(plan);
  };
  const restartClientNow = () => {
    setDeceivePrompt(null);
    if (data.activeId) void switchToAccount(data.activeId);
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
  const otherMode: View = toolsOpen ? "accounts" : inChampSelect ? "runes" : "champions";
  const switchLabel = toolsOpen ? "Switch to accounts" : "Switch to champion lookup";
  // Runes, champions and settings wear the Fullsized App's identity (flyout.css); the accounts screens keep the original look.
  const themed = !fullApp && (toolsOpen || view === "settings");
  // The top bar wears the new identity on every flyout screen, accounts included, so it does not change look between screens.
  const modeName = (mode: string) => fullApp ? mode.toUpperCase() : mode[0].toUpperCase() + mode.slice(1);

  return {
    data, view, inChampSelect, modeSpin, setModeSpin, busy, switchingAccountId, error, setError, name,
    setName, editing, setEditing, pendingRemove, setPendingRemove, repairableId, setRepairableId,
    repairingId, setRepairingId, accountMenu, setAccountMenu, useDeceive, koreanFont, ltkInstalling,
    ltkInstallError, riotExe, setRiotExe, addStage, setAddStage, detect, setDetectRound, setDetectedPrompt,
    remoteTransport, setRemoteTransport, settingsTab, setSettingsTab, remoteQrRequest, startOnStartup,
    runes, runesLoading, runesBusy, runesError, shopNotice, runeSession, native, sync, showError,
    loadRunes, importItemBuild, applyRunes, pickSpell, allowLanFirewall, setImportItemsWithRunes,
    setApplySpellsWithRunes, loadProBuilds, loadKeystoneBuild, loadMatchup, championApi, setAutoApply,
    setRuneTier, setNotifications, setReadyCheckNotifications, setKoreanFontSetting, installLtkManager,
    setRemote, showRemoteQr, dismissReconnect, setStartOnStartupSetting, action, navigate, isBusy,
    openAccountMenu, menuAccount, switchToAccount, startRepair, visibleDetectedPrompt, displayedActiveId,
    pageTitle, footerText, footerLoggedIn, conflictingSavedAccount, saveDetected, manualPath,
    configuredPath, applySettings, changeDeceive, deceivePrompt, setDeceivePrompt, restartClientNow, changeRiotPathMode, commitRiotPath, applyHotkey, browseRiotPath,
    toolsOpen, brandMode, otherMode, switchLabel, themed, modeName,
  };
}

export type AppController = ReturnType<typeof useAppController>;
