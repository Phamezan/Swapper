import { emit } from "@tauri-apps/api/event";
import { extra } from "./full-app/champ-select-mock";
import type { Account } from "./app/types";

export type FlyoutMockOptions = {
  accounts?: Account[];
  initialActiveId?: string | null;
  hotkey?: string | null;
  overrides?: (ctx: { getActiveId: () => string | null }) => Record<string, (args: Record<string, any>) => unknown>;
  onSwitch?: (id: string) => void;
};

const defaultAccount = (id: string, name: string, region: string): Account => ({
  id, name, riotId: `${name}#EUW`, nickname: null, platform: "EUW1", region, profileIconId: null, iconDataUrl: null,
});

export function installFlyoutMock(options: FlyoutMockOptions = {}) {
  const params = new URLSearchParams(location.search);
  const accounts = options.accounts ? options.accounts.map(a => ({ ...a })) : [
    defaultAccount("a", "Main account", "EUW"),
    defaultAccount("b", "Smurf", "EUW"),
    defaultAccount("c", "Practice", "NA"),
  ];
  const remote = {
    enabled: true, state: "available", address: "http://192.168.1.20:7878", tailscaleAddress: null, lanAddress: "192.168.1.20", lanMessage: null,
    lanFirewallNeeded: false, message: null, tailscaleInstalled: false, tailscaleRunning: false, dnsName: null, localAddress: null,
    leagueRunning: true, lcuConnected: true, lanReconnectNeeded: params.has("reconnect"), lanReconnectOldAddress: null, lanReconnectNewAddress: null, lanReconnectSince: null,
  };
  const initialHotkey = options.hotkey !== undefined ? options.hotkey : "Ctrl+Shift+S";
  const state = {
    accounts,
    activeId: options.initialActiveId !== undefined ? options.initialActiveId : (accounts[0]?.id ?? null),
    isSwitching: false,
    useDeceive: false, riotExe: null, riotDetected: true, deceiveDetected: true, autoApplyTopPreset: true, runeTier: "emerald_plus",
    applySpellsWithRunes: true, importItemsWithRunes: true, hotkey: initialHotkey, hotkeyActive: Boolean(initialHotkey), notificationsEnabled: true,
    readyCheckNotifications: true, koreanFont: false, customMods: false, customModsPaused: false, customModsRiskAck: false, modsRegionBlocked: false,
    ltkInstalled: false, ltkVersion: null, ltkOutdated: false, ltkLatestVersion: null, ltkNeedsSetup: false, ltkNotice: null, remote,
  };
  const none = () => null;
  const champSelect = { phase: params.get("fixture") ? "ChampSelect" : "None", championId: 266, championName: "Aatrox", position: "top", locked: params.get("fixture") === "locked" };

  const handlers: typeof extra = {
    get_state: () => ({ ...state, accounts: state.accounts.map(a => ({ ...a })) }),
    window_ready: none, hide_flyout: none, onboarding_pending: () => false, set_onboarded: none, window_settings: () => ({}),
    champ_select_status: () => champSelect,
    client_health: () => params.get("client") ?? "ok",
    detect_account_identity: () => ({ state: "notRunning" }),
    "plugin:autostart|is_enabled": () => false,
    "plugin:autostart|enable": none, "plugin:autostart|disable": none,
    live_game_active: () => false, live_game_auto_open: () => false, live_game_live: () => null, live_game_view: () => null,
    get_overlay_settings: () => ({ enabled: true, opacity: 80, scale: 100, objectives: true, gold: true, skills: true, levelUp: true, cooldowns: true }),
    set_overlay_settings: (a) => a.settings,
    list_paired_lan_devices: () => [{ id: "p1", name: "Phone", pairedAt: Math.floor(Date.now() / 1000) - 86400, lastSeen: Math.floor(Date.now() / 1000) - 3600, connected: true }],
    update_status: () => ({ kind: "upToDate", version: "1.0.21" }),
    run_doctor: () => [],
    set_remote_enabled: (a) => ({ ...state.remote, enabled: Boolean(a.enabled), state: a.enabled ? "available" : "disabled" }),
    set_notifications_enabled: (a) => { state.notificationsEnabled = Boolean(a.enabled); return { ...state }; },
    set_ready_check_notifications: (a) => { state.readyCheckNotifications = Boolean(a.enabled); return { ...state }; },
    set_auto_apply_top_preset: (a) => { state.autoApplyTopPreset = Boolean(a.enabled); return { ...state }; },
    set_apply_spells_with_runes: (a) => { state.applySpellsWithRunes = Boolean(a.enabled); return { ...state }; },
    set_import_items_with_runes: (a) => { state.importItemsWithRunes = Boolean(a.enabled); return { ...state }; },
    set_korean_font: (a) => { state.koreanFont = Boolean(a.enabled); return { ...state }; },
    set_rune_tier: (a) => { state.runeTier = String(a.tier ?? state.runeTier); return { ...state }; },
    set_launch_target: (a) => {
      state.accounts = state.accounts.map(x => x.id === a.id ? { ...x, launchTarget: a.target } : x);
      return { ...state };
    },
    set_nickname: (a) => {
      const nick = String(a.name ?? "").trim() || null;
      state.accounts = state.accounts.map(x => x.id === a.id ? { ...x, nickname: nick, name: nick ?? (x.riotId?.split("#")[0] || x.name) } : x);
      return { ...state };
    },
    remove_account: (a) => {
      state.accounts = state.accounts.filter(x => x.id !== a.id);
      if (state.activeId === a.id) state.activeId = state.accounts[0]?.id ?? null;
      return { ...state };
    },
    reorder_accounts: (a) => {
      state.accounts = (a.ids as string[]).map(id => state.accounts.find(x => x.id === id)!).filter(Boolean);
      return { ...state };
    },
    switch_account: (a) => {
      const id = String(a.id);
      const account = state.accounts.find(x => x.id === id);
      const delay = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ? 40 : 420;
      setTimeout(() => void emit("switch_started", { id, name: account?.name ?? "" }), 0);
      setTimeout(() => {
        state.activeId = id;
        options.onSwitch?.(id);
        void emit("switch_done", { id, view: { ...state, accounts: state.accounts.map(x => ({ ...x })) } });
      }, delay);
      return null;
    },
    begin_add: () => ({ ...state }), open_riot: none, create_account_shortcut: none, begin_repair: none,
    restart_plan: () => "restart",
    probe_remote: () => state.remote,
    save_settings: (a) => {
      if (typeof a.useDeceive === "boolean") state.useDeceive = a.useDeceive;
      if ("riotExe" in a) state.riotExe = a.riotExe ?? null;
      return { ...state };
    },
    set_hotkey: (a) => { state.hotkey = a.shortcut ?? null; return { ...state }; },
    create_lan_pairing_url: () => "http://192.168.1.20:7878/pair?x=1",
    get_champion_overview: (a) => ({
      championId: a.championId, position: "top", unavailable: false, damage: { physical: 62, magic: 31, trueDamage: 7 },
      stats: { tier: "S+", rank: 1, rankTotal: 58, winPct: 52.6, avgWinPct: 50.1, delta: 2.5, pickPct: 9, banPct: 4, games: 24000, patch: "16.21" }, build: null,
    }),
    get_champion_counters: (a) => ({
      championId: a.championId, championName: "Aatrox", position: "top", unavailable: false, stale: false, updatedAt: null,
      bestInto: [[67, "Vayne", 46.1], [133, "Quinn", 46.8], [17, "Teemo", 47.2]].map(([championId, name, winPct]) => ({ championId, name, winPct, delta: -4, games: 2400, lowSample: false })),
      beats: [[54, "Malphite", 55.0], [2, "Olaf", 54.1], [98, "Shen", 53.8]].map(([championId, name, winPct]) => ({ championId, name, winPct, delta: 4, games: 1800, lowSample: false })),
    }),
    get_duo_list: () => ({ unavailable: false, stale: false, updatedAt: null, rows: [] }),
    rank_icon: () => "", team_icon: () => "", set_full_app_enabled: none,
  };

  Object.assign(extra, handlers);
  if (options.overrides) Object.assign(extra, options.overrides({ getActiveId: () => state.activeId }));
  if (params.has("error")) {
    extra.set_notifications_enabled = () => { throw new Error("Couldn't save the setting. Try again."); };
    extra.switch_account = () => { throw new Error("Couldn't switch accounts. Try again."); };
  }
  if (params.get("fixture")) setTimeout(() => void emit("champ_select", champSelect), 600);

  return {
    syncActiveId(id: string) {
      if (state.activeId === id || !state.accounts.some(a => a.id === id)) return;
      state.activeId = id;
      void emit("accounts_changed");
    },
  };
}
