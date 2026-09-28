import { useState } from "react";
import { Info } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { HotkeySetting } from "./HotkeySetting";
import { NotificationSettings } from "./NotificationSettings";

type Props = {
  useDeceive: boolean;
  onUseDeceiveChange: (checked: boolean) => void;
  startOnStartup: boolean;
  onStartOnStartupChange: (enabled: boolean) => void;
  /** True once the user picks Manual, or when a custom path is configured. */
  manualPath: boolean;
  riotExe: string;
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
};

export function GeneralSettings({
  useDeceive,
  onUseDeceiveChange,
  startOnStartup,
  onStartOnStartupChange,
  manualPath,
  riotExe,
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
}: Props) {
  const [openInfo, setOpenInfo] = useState<string | null>(null);
  const toggle = (key: string) => setOpenInfo(openInfo === key ? null : key);

  return (
    <>
      <div className="setting-row">
        <div className="setting-copy">
          <strong>Start on startup</strong>
          <button className="info-button" aria-label="About Start on startup" aria-expanded={openInfo === "startup"} onClick={() => toggle("startup")}><Info size={13} /></button>
          {openInfo === "startup" && <p>Launch Swapper with Windows. It starts in the tray, without opening the flyout.</p>}
        </div>
        <Switch checked={startOnStartup} onCheckedChange={onStartOnStartupChange} aria-label="Start on startup" />
      </div>

      <div className="setting-row">
        <div className="setting-copy">
          <strong>Launch through Deceive</strong>
          <button className="info-button" aria-label="About Launch through Deceive" aria-expanded={openInfo === "deceive"} onClick={() => toggle("deceive")}><Info size={13} /></button>
          {openInfo === "deceive" && <p>Start League with Deceive’s offline presence. Included with Swapper.</p>}
        </div>
        <Switch checked={useDeceive} onCheckedChange={onUseDeceiveChange} aria-label="Launch through Deceive" />
      </div>

      <p className="field-label">RIOT CLIENT PATH</p>
      <div className="segmented" role="group" aria-label="Riot Client path">
        <button type="button" className={manualPath ? "" : "is-on"} aria-pressed={!manualPath} onClick={() => onPathModeChange("auto")}>Auto</button>
        <button type="button" className={manualPath ? "is-on" : ""} aria-pressed={manualPath} onClick={() => onPathModeChange("manual")}>Manual</button>
      </div>
      {manualPath && (
        <div className="path-input">
          <Input id="riot-path" placeholder="Path to RiotClientServices.exe" value={riotExe} onChange={(e) => onRiotExeChange(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter") e.currentTarget.blur(); }} onBlur={(e) => onRiotPathCommit(e.currentTarget.value.trim() || null)} />
          <Button variant="outline" onMouseDown={(e) => e.preventDefault()} onClick={onBrowseRiotPath}>Browse…</Button>
        </div>
      )}

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
