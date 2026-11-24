import { useState, type ReactNode } from "react";
import { Info } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { HotkeySetting } from "./HotkeySetting";
import { NotificationSettings } from "./NotificationSettings";
import { RUN_EVENT } from "../onboarding/onboardingModel";
import { WindowModeSettings } from "../full-app/WindowModeSettings";
import { LaunchTargetPicker } from "./LaunchTargetPicker";
import type { LaunchTarget } from "../app/types";

type Props = {
  useDeceive: boolean;
  onUseDeceiveChange: (checked: boolean) => void;
  /** Restart prompt rendered under the Deceive row. */
  deceiveNotice?: ReactNode;
  startOnStartup: boolean;
  onStartOnStartupChange: (enabled: boolean) => void;
  /** True once the user picks Manual, or when a custom path is configured. */
  manualPath: boolean;
  riotExe: string;
  riotDetected: boolean;
  onRiotExeChange: (value: string) => void;
  /** Persists the mode; Auto also clears and saves the path as undetected. */
  onPathModeChange: (mode: "auto" | "manual") => void;
  /** Persists the edited path; null switches back to Auto. */
  onRiotPathCommit: (value: string | null) => void;
  onBrowseRiotPath: () => void;
  hotkey: string | null;
  hotkeyActive: boolean;
  busy: boolean;
  onSetHotkey: (value: string | null) => Promise<void>;
  notifications: boolean;
  readyCheck: boolean;
  onNotificationsChange: (enabled: boolean) => void;
  onReadyCheckChange: (enabled: boolean) => void;
  launchTarget: LaunchTarget;
  onLaunchTargetChange: (target: LaunchTarget) => void;
  koreanFont: boolean;
  customMods: boolean;
  customModsRiskAck: boolean;
  onCustomModsChange: (enabled: boolean, riskAck: boolean) => void;
  ltkInstalled: boolean;
  ltkOutdated: boolean;
  ltkNeedsSetup: boolean;
  /** What a queued LTK Manager change waits for. */
  ltkNotice: string | null;
  ltkInstalling: boolean;
  ltkInstallError: string | null;
  onInstallLtk: () => void;
};

export function GeneralSettings({
  useDeceive,
  onUseDeceiveChange,
  deceiveNotice,
  startOnStartup,
  onStartOnStartupChange,
  manualPath,
  riotExe,
  riotDetected,
  onRiotExeChange,
  onPathModeChange,
  onRiotPathCommit,
  onBrowseRiotPath,
  hotkey,
  hotkeyActive,
  busy,
  onSetHotkey,
  notifications,
  readyCheck,
  onNotificationsChange,
  onReadyCheckChange,
  launchTarget,
  onLaunchTargetChange,
  koreanFont,
  customMods,
  customModsRiskAck,
  onCustomModsChange,
  ltkInstalled,
  ltkOutdated,
  ltkNeedsSetup,
  ltkNotice,
  ltkInstalling,
  ltkInstallError,
  onInstallLtk,
}: Props) {
  const [openInfo, setOpenInfo] = useState<string | null>(null);
  const [confirmingMods, setConfirmingMods] = useState(false);
  const changeCustomMods = (checked: boolean) => {
    if (checked && !customModsRiskAck) { setConfirmingMods(true); return; }
    onCustomModsChange(checked, false);
  };
  const toggle = (key: string) => setOpenInfo(openInfo === key ? null : key);
  const ltkBox = ltkNotice !== null || !ltkInstalled || ltkOutdated || (ltkNeedsSetup && (koreanFont || customMods));
  const ltkAction = !ltkInstalled
    ? (ltkInstalling ? "Installing…" : "Install")
    : ltkOutdated
      ? (ltkInstalling ? "Updating…" : "Update")
      : (ltkInstalling ? "Setting up…" : "Set up LTK Manager");

  return (
    <>
      <WindowModeSettings />
      <div className="setting-row">
        <div className="setting-copy">
          <label htmlFor="startup-switch">Start on startup</label>
          <button className="info-button" aria-label="About Start on startup" aria-expanded={openInfo === "startup"} onClick={() => toggle("startup")}><Info size={13} /></button>
          {openInfo === "startup" && <p>Launch with Windows, in the tray.</p>}
        </div>
        <Switch id="startup-switch" checked={startOnStartup} disabled={busy} onCheckedChange={onStartOnStartupChange} aria-label="Start on startup" />
      </div>

      <div className="setting-row">
        <div className="setting-copy">
          <label htmlFor="deceive-switch">Launch through Deceive</label>
          <button className="info-button" aria-label="About Launch through Deceive" aria-expanded={openInfo === "deceive"} onClick={() => toggle("deceive")}><Info size={13} /></button>
          {openInfo === "deceive" && <p>Start League with Deceive’s offline presence.</p>}
        </div>
        <Switch id="deceive-switch" checked={useDeceive} disabled={busy} onCheckedChange={onUseDeceiveChange} aria-label="Launch through Deceive" />
      </div>
      {deceiveNotice}

      <p className="field-label">After switching, open</p>
      <LaunchTargetPicker label="After switching, open" value={launchTarget} disabled={busy} onChange={(target) => onLaunchTargetChange(target ?? "league")} />

      <div className="setting-row">
        <div className="setting-copy">
          <label htmlFor="custom-mods-switch">Custom mods</label>
          <p>Enables a marketplace section where you can browse and install custom mods.</p>
        </div>
        <Switch id="custom-mods-switch" checked={customMods} disabled={busy} onCheckedChange={changeCustomMods} aria-label="Custom mods" />
      </div>
      {confirmingMods && <div className="deceive-notice is-warn" role="alertdialog" aria-label="Custom mods warning">
        <p>Mods can break after patches and are against Riot&apos;s rules. Use at your own risk.</p>
        <div>
          <Button variant="outline" onClick={() => setConfirmingMods(false)}>Cancel</Button>
          <Button onClick={() => { setConfirmingMods(false); onCustomModsChange(true, true); }}>Turn on</Button>
        </div>
      </div>}
      {ltkBox && <div className="deceive-notice is-warn" role="status">
        {ltkNotice
          ? <p>{ltkNotice}</p>
          : <>
            <p>{!ltkInstalled ? "LTK Manager applies the Korean font and custom mods." : ltkOutdated ? "LTK Manager is out of date." : "LTK Manager isn't set up yet."}</p>
            <p>Swapper sets LTK Manager to run in the background and apply your mods every game.</p>
            <div><Button variant="outline" disabled={ltkInstalling} onClick={onInstallLtk}>{ltkAction}</Button></div>
          </>}
        {ltkInstallError && <p>{ltkInstallError}</p>}
      </div>}

      <p className="field-label">Riot Client path</p>
      <div className="segmented" role="group" aria-label="Riot Client path">
        <button type="button" className={manualPath ? "" : "is-on"} aria-pressed={!manualPath} onClick={() => onPathModeChange("auto")}>Auto</button>
        <button type="button" className={manualPath ? "is-on" : ""} aria-pressed={manualPath} onClick={() => onPathModeChange("manual")}>Manual</button>
      </div>
      {!manualPath && <p className="path-status" role="status"><span className={`status-dot ${riotDetected ? "good" : "idle"}`} />{riotDetected ? "Riot Client detected" : "Riot Client not found. Choose Manual to set its path."}</p>}
      {manualPath && (
        <div className="path-input">
          <Input id="riot-path" placeholder="Path to RiotClientServices.exe" value={riotExe} onChange={(e) => onRiotExeChange(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter") e.currentTarget.blur(); }} onBlur={(e) => onRiotPathCommit(e.currentTarget.value.trim() || null)} />
          <Button variant="outline" onMouseDown={(e) => e.preventDefault()} onClick={onBrowseRiotPath}>Browse…</Button>
        </div>
      )}

      <div className="setting-row">
        <div className="setting-copy">
          <strong>Setup</strong>
          <p>Choose again how you use Swapper, the overlay and the extras.</p>
        </div>
        <Button variant="outline" onClick={() => window.dispatchEvent(new Event(RUN_EVENT))}>Run setup again</Button>
      </div>

      <HotkeySetting hotkey={hotkey} active={hotkeyActive} disabled={busy} onSet={onSetHotkey} />

      <NotificationSettings
        notifications={notifications}
        readyCheck={readyCheck}
        busy={busy}
        onNotificationsChange={onNotificationsChange}
        onReadyCheckChange={onReadyCheckChange}
      />
    </>
  );
}
