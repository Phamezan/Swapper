import { useRef } from "react";
import type { AppState } from "../App";
import { GeneralSettings } from "./GeneralSettings";
import { RuneSettings } from "./RuneSettings";
import { RemoteSettings } from "./RemoteSettings";
import { AboutSettings } from "./AboutSettings";

const TABS = [
  { id: "general", label: "General" },
  { id: "runes", label: "Runes" },
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
  onAutoApplyChange: (enabled: boolean) => void;
  onApplySpellsChange: (enabled: boolean) => void;
  onImportItemsChange: (enabled: boolean) => void;
  onRuneTierChange: (tier: string) => void;
  remoteTransport: "lan" | "tailscale";
  onRemoteTransportChange: (transport: "lan" | "tailscale") => void;
  onRemoteToggle: (enabled: boolean) => void;
  onError: (reason: unknown) => void;
};

/** The Settings view: one compact tab bar, only the active tab's content
 *  mounted. The bar sticks to the top of the scroll area; the panel below it
 *  scrolls. Arrow keys follow the tabs pattern with a roving tabindex. */
export function SettingsView(props: Props) {
  const { tab, onTabChange } = props;
  const tabRefs = useRef(new Map<SettingsTab, HTMLButtonElement | null>());

  const selectTab = (id: SettingsTab) => {
    onTabChange(id);
    tabRefs.current.get(id)?.focus();
  };

  const onTabKeyDown = (event: React.KeyboardEvent<HTMLButtonElement>) => {
    const index = TABS.findIndex(({ id }) => id === tab);
    let next = -1;
    if (event.key === "ArrowRight") next = (index + 1) % TABS.length;
    else if (event.key === "ArrowLeft") next = (index + TABS.length - 1) % TABS.length;
    else if (event.key === "Home") next = 0;
    else if (event.key === "End") next = TABS.length - 1;
    if (next >= 0) {
      event.preventDefault();
      selectTab(TABS[next].id);
    }
  };

  return (
    <>
      <div className="settings-tabbar">
        <div className="settings-tabs" role="tablist" aria-label="Settings sections">
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
        className="settings-content"
        role="tabpanel"
        id={`settings-panel-${tab}`}
        aria-labelledby={`settings-tab-${tab}`}
        tabIndex={0}
      >
        {tab === "general" && <GeneralSettings
          useDeceive={props.useDeceive}
          onUseDeceiveChange={props.onUseDeceiveChange}
          startOnStartup={props.startOnStartup}
          onStartOnStartupChange={props.onStartOnStartupChange}
          manualPath={props.manualPath}
          riotExe={props.riotExe}
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
        {tab === "remote" && <RemoteSettings
          remote={props.state.remote}
          transport={props.remoteTransport}
          onTransportChange={props.onRemoteTransportChange}
          onToggle={props.onRemoteToggle}
          onError={props.onError}
        />}
        {tab === "about" && <AboutSettings remoteTransport={props.remoteTransport} />}
      </div>
    </>
  );
}
