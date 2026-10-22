import { useEffect, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Switch } from "@/components/ui/switch";
import { LiveGameSettings } from "./LiveGameSettings";

type WindowSettings = { fullApp: boolean; lowMemory: boolean };

/** Settings → General rows for Fullsized App and, in flyout mode, low memory mode. */
export function WindowModeSettings() {
  const [settings, setSettings] = useState<WindowSettings | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  useEffect(() => {
    if (!isTauri()) return;
    let live = true;
    const stop = listen<WindowSettings>("window_settings", ({ payload }) => { if (live) setSettings(payload); });
    void invoke<WindowSettings>("window_settings").then(value => { if (live) setSettings(value); }).catch(reason => { if (live) setError(String(reason)); });
    return () => { live = false; void stop.then(unlisten => unlisten()); };
  }, []);
  async function change(command: string, enabled: boolean) {
    setBusy(true); setError("");
    // Switching Fullsized App closes this window, so nothing after it may matter.
    try { await invoke(command, { enabled }); }
    catch (reason) { setError(String(reason)); }
    finally { setBusy(false); }
  }
  const disabled = busy || !settings;
  return <>
    <div className="setting-row">
      <div className="setting-copy">
        <label htmlFor="full-app-switch">Fullsized App</label>
        <p>Full window instead of the tray flyout.</p>
        {error && <p role="alert">{error}</p>}
      </div>
      <Switch id="full-app-switch" checked={settings?.fullApp ?? false} disabled={disabled} onCheckedChange={value => void change("set_full_app_enabled", value)} aria-label="Fullsized App" />
    </div>
    {settings?.fullApp && <LiveGameSettings />}
    {settings && !settings.fullApp && <div className="setting-row">
      <div className="setting-copy">
        <label htmlFor="low-memory-switch">Low memory mode</label>
        <p>Frees the flyout's memory while hidden. Reopens slower.</p>
      </div>
      <Switch id="low-memory-switch" checked={settings.lowMemory} disabled={disabled} onCheckedChange={value => void change("set_low_memory_mode", value)} aria-label="Low memory mode" />
    </div>}
  </>;
}
