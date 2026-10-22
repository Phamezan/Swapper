import type { ComponentType, ComponentProps } from "react";
import type { RunesPanel } from "../runes/RunesPanel";
import type { SettingsView } from "../settings/SettingsView";
import type { AccountCardsProps } from "../full-app/AccountCards";

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
export type DetectedIdentity = {
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
export type DetectState =
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
export type ChampSelectStatus = {
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

