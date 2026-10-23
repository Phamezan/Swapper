import type { ReactNode } from "react";
import { useRef } from "react";
import type { AppState } from "../App";
import { GeneralSettings } from "../settings/GeneralSettings";
import { FullRuneSettings as RuneSettings } from "./FullRuneSettings";
import { OverlayCards as OverlaySettings } from "./OverlayCards";
import { RemoteSettings } from "../settings/RemoteSettings";
import { AboutSettings } from "../settings/AboutSettings";
import { LookPreview } from './LookPreview';

const TABS = [
  { id: "general", label: "General" },
  { id: "runes", label: "Runes" },
  { id: "overlay", label: "Overlay" },
  { id: "remote", label: "Remote" },
  { id: "about", label: "About" },
] as const;

export type SettingsTab = (typeof TABS)[number]["id"];

type Props = {
  state: AppState;
  busy: boolean;
  tab: SettingsTab;
  onTabChange: (tab: SettingsTab) => void;
  useDeceive: boolean;
  onUseDeceiveChange: (checked: boolean) => void;
  deceiveNotice?: ReactNode;
  startOnStartup: boolean;
  onStartOnStartupChange: (enabled: boolean) => void;
  manualPath: boolean;
  riotExe: string;
  onRiotExeChange: (value: string) => void;
  onPathModeChange: (mode: "auto" | "manual") => void;
  onRiotPathCommit: (value: string | null) => void;
  onBrowseRiotPath: () => void;
  onSetHotkey: (value: string | null) => Promise<void>;
  onNotificationsChange: (enabled: boolean) => void;
  onReadyCheckChange: (enabled: boolean) => void;
  koreanFont: boolean;
  onKoreanFontChange: (enabled: boolean) => void;
  ltkInstalled: boolean;
  ltkInstalling: boolean;
  ltkInstallError: string | null;
  onInstallLtk: () => void;
  onAutoApplyChange: (enabled: boolean) => void;
  onApplySpellsChange: (enabled: boolean) => void;
  onImportItemsChange: (enabled: boolean) => void;
  onAllowLanFirewall: () => Promise<void>;
  onRuneTierChange: (tier: string) => void;
  remoteTransport: "lan" | "tailscale";
  onRemoteTransportChange: (transport: "lan" | "tailscale") => void;
  onRemoteToggle: (enabled: boolean) => void;
  /** Bumped to ask the Remote tab to open its QR panel. */
  qrRequest: number;
  onError: (reason: unknown) => void;
};

/** Only the tab content scrolls; the heading and tabs stay in place.
 *  Arrow keys follow the tabs pattern with a roving tabindex. */
export function SettingsPage(props: Props) {
  const { tab, onTabChange } = props;
  const tabRefs = useRef(new Map<SettingsTab, HTMLButtonElement | null>());

  const selectTab = (id: SettingsTab) => {
    onTabChange(id);
    tabRefs.current.get(id)?.focus();
  };

  const onTabKeyDown = (event: React.KeyboardEvent<HTMLButtonElement>) => {
    const index = TABS.findIndex(({ id }) => id === tab);
    let next = -1;
    if (event.key === "ArrowRight" || event.key === "ArrowDown") next = (index + 1) % TABS.length;
    else if (event.key === "ArrowLeft" || event.key === "ArrowUp") next = (index + TABS.length - 1) % TABS.length;
    else if (event.key === "Home") next = 0;
    else if (event.key === "End") next = TABS.length - 1;
    if (next >= 0) {
      event.preventDefault();
      selectTab(TABS[next].id);
    }
  };

  return (
    <div className="fa-settings-layout">
      <div className="fa-settings-nav">
        <div className="fa-settings-tabs" role="tablist" aria-orientation="vertical" aria-label="Settings sections">
          {TABS.map(({ id, label }) => (
            <button
              key={id}
              ref={(element) => {
                tabRefs.current.set(id, element);
              }}
              type="button"
              role="tab"
              id={`settings-tab-${id}`}
              className="settings-tab"
              aria-selected={tab === id}
              aria-controls={`settings-panel-${id}`}
              tabIndex={tab === id ? 0 : -1}
              onClick={() => onTabChange(id)}
              onKeyDown={onTabKeyDown}
            >
              {label}
            </button>
          ))}
        </div>
      </div>
      <div
        key={tab}
        className="fa-settings-content"
        role="tabpanel"
        id={`settings-panel-${tab}`}
        aria-labelledby={`settings-tab-${tab}`}
        tabIndex={0}
      >
        <header className="fa-settings-heading"><h2>{TABS.find(item => item.id === tab)?.label}</h2><p>{({general: "Startup, Riot Client and alerts", runes: "Applied every draft", overlay: "In-game info", remote: "Draft from your phone", about: "Updates and diagnostics"})[tab]}</p></header>
        <div className={`fa-settings-group fa-settings-${tab}`}>
        {tab === 'general' && <LookPreview />}
        {tab === "general" && <GeneralSettings
          useDeceive={props.useDeceive}
          onUseDeceiveChange={props.onUseDeceiveChange}
          deceiveNotice={props.deceiveNotice}
          startOnStartup={props.startOnStartup}
          onStartOnStartupChange={props.onStartOnStartupChange}
          manualPath={props.manualPath}
          riotExe={props.riotExe}
          riotDetected={props.state.riotDetected}
          onRiotExeChange={props.onRiotExeChange}
          onPathModeChange={props.onPathModeChange}
          onRiotPathCommit={props.onRiotPathCommit}
          onBrowseRiotPath={props.onBrowseRiotPath}
          hotkey={props.state.hotkey}
          hotkeyActive={props.state.hotkeyActive}
          busy={props.busy}
          onSetHotkey={props.onSetHotkey}
          notifications={props.state.notificationsEnabled}
          readyCheck={props.state.readyCheckNotifications}
          onNotificationsChange={props.onNotificationsChange}
          onReadyCheckChange={props.onReadyCheckChange}
          koreanFont={props.koreanFont}
          onKoreanFontChange={props.onKoreanFontChange}
          ltkInstalled={props.ltkInstalled}
          ltkInstalling={props.ltkInstalling}
          ltkInstallError={props.ltkInstallError}
          onInstallLtk={props.onInstallLtk}
        />}
        {tab === "runes" && <RuneSettings
          autoApply={props.state.autoApplyTopPreset}
          onAutoApplyChange={props.onAutoApplyChange}
          applySpells={props.state.applySpellsWithRunes}
          onApplySpellsChange={props.onApplySpellsChange}
          importItems={props.state.importItemsWithRunes}
          onImportItemsChange={props.onImportItemsChange}
          runeTier={props.state.runeTier}
          busy={props.busy}
          onRuneTierChange={props.onRuneTierChange}
        />}
        {tab === "overlay" && <OverlaySettings />}
        {tab === "remote" && <RemoteSettings
          remote={props.state.remote}
          transport={props.remoteTransport}
          onTransportChange={props.onRemoteTransportChange}
          onToggle={props.onRemoteToggle}
          onAllowFirewall={props.onAllowLanFirewall}
          qrRequest={props.qrRequest}
          onError={props.onError}
        />}
        {tab === "about" && <AboutSettings remoteTransport={props.remoteTransport} />}
        </div>
      </div>
    </div>
  );
}
