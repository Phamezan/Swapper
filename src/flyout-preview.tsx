// Dev-only preview of the tray flyout (App) with mocked Tauri IPC: `npm run dev`, then open
// /src/flyout-preview.html at a 400x640 window. ?fixture=planning|hover|locked picks the champion select
// state; ?error makes a toggle and a switch fail (error banner). Never part of the app build. Fixture data only.
import { createRoot } from "react-dom/client";
import { emit } from "@tauri-apps/api/event";
import { extra } from "./full-app/champ-select-mock";
import App from "./App";

const params = new URLSearchParams(location.search);
const account = (id: string, name: string, region: string) => ({ id, name, riotId: `${name}#EUW`, nickname: null, platform: "EUW1", region, profileIconId: null, iconDataUrl: null });
const remote = {
  enabled: true, state: "available", address: "http://192.168.1.20:7878", tailscaleAddress: null, lanAddress: "192.168.1.20", lanMessage: null,
  lanFirewallNeeded: false, message: null, tailscaleInstalled: false, tailscaleRunning: false, dnsName: null, localAddress: null,
  leagueRunning: true, lcuConnected: true, lanReconnectNeeded: params.has("reconnect"), lanReconnectOldAddress: null, lanReconnectNewAddress: null, lanReconnectSince: null,
};
const state = {
  accounts: [account("a", "Main account", "EUW"), account("b", "Smurf", "EUW"), account("c", "Practice", "NA")], activeId: "a", isSwitching: false,
  useDeceive: false, riotExe: null, riotDetected: true, deceiveDetected: true, autoApplyTopPreset: true, runeTier: "emerald_plus",
  applySpellsWithRunes: true, importItemsWithRunes: true, hotkey: "Ctrl+Shift+S", hotkeyActive: true, notificationsEnabled: true,
  readyCheckNotifications: true, koreanFont: false, ltkInstalled: false, remote,
};
const none = () => null;
const champSelect = { phase: params.get("fixture") ? "ChampSelect" : "None", championId: 266, championName: "Aatrox", position: "top", locked: params.get("fixture") === "locked" };
const handlers: typeof extra = {
  get_state: () => state, window_ready: none, onboarding_pending: () => false, set_onboarded: none, window_settings: () => ({}),
  champ_select_status: () => champSelect,
  client_health: () => params.get("client") ?? "ok",
  detect_account_identity: () => ({ state: "notRunning" }), "plugin:autostart|is_enabled": () => false,
  live_game_active: () => false, live_game_auto_open: () => false, live_game_live: () => null, live_game_view: () => null,
  get_overlay_settings: () => ({ enabled: true, opacity: 80, scale: 100, objectives: true, gold: true, skills: true, levelUp: true, cooldowns: true }),
  list_paired_lan_devices: () => [{ id: "p1", name: "Phone", pairedAt: Math.floor(Date.now() / 1000) - 86400, lastSeen: Math.floor(Date.now() / 1000) - 3600, connected: true }],
  update_status: () => ({ kind: "upToDate", version: "0.9.0" }), run_doctor: () => [], set_remote_enabled: () => state,
  set_notifications_enabled: () => state, set_ready_check_notifications: () => state, set_auto_apply_top_preset: () => state,
  set_apply_spells_with_runes: () => state, set_import_items_with_runes: () => state, set_korean_font: () => state, set_rune_tier: () => state,
  restart_plan: () => "restart" /* clientctl */, probe_remote: () => state.remote, save_settings: () => state, set_hotkey: () => state, create_lan_pairing_url: () => "http://192.168.1.20:7878/pair?x=1",
  get_champion_overview: (a) => ({ championId: a.championId, position: "top", unavailable: false, damage: { physical: 62, magic: 31, trueDamage: 7 },
    stats: { tier: "S+", rank: 1, rankTotal: 58, winPct: 52.6, avgWinPct: 50.1, delta: 2.5, pickPct: 9, banPct: 4, games: 24000, patch: "16.21" }, build: null }),
  get_champion_counters: (a) => ({ championId: a.championId, championName: "Aatrox", position: "top", unavailable: false, stale: false, updatedAt: null,
    bestInto: [[67, "Vayne", 46.1], [133, "Quinn", 46.8], [17, "Teemo", 47.2]].map(([championId, name, winPct]) => ({ championId, name, winPct, delta: -4, games: 2400, lowSample: false })),
    beats: [[54, "Malphite", 55.0], [2, "Olaf", 54.1], [98, "Shen", 53.8]].map(([championId, name, winPct]) => ({ championId, name, winPct, delta: 4, games: 1800, lowSample: false })) }),
  get_duo_list: () => ({ unavailable: false, stale: false, updatedAt: null, rows: [] }),
  rank_icon: () => "", team_icon: () => "", set_full_app_enabled: none,
};
Object.assign(extra, handlers);
if (params.has("error")) {
  extra.set_notifications_enabled = () => { throw new Error("Couldn't save the setting. Try again."); };
  extra.switch_account = () => { throw new Error("Couldn't switch accounts. Try again."); };
}

if (params.get("fixture")) setTimeout(() => void emit("champ_select", champSelect), 600);
createRoot(document.getElementById("root")!).render(<App />);
