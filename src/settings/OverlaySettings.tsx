import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Switch } from "@/components/ui/switch";

type Settings = {
  enabled: boolean;
  gold: boolean;
  objectives: boolean;
  skills: boolean;
  goldHotkey: number;
};

export function OverlaySettings() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let mounted = true;
    invoke<Settings>("get_overlay_settings").then(
      value => { if (mounted) setSettings(value); },
      reason => { if (mounted) setError(String(reason)); },
    );
    return () => { mounted = false; };
  }, []);

  async function update(change: Partial<Settings>) {
    if (!settings || busy) return;
    setBusy(true);
    setError(null);
    try {
      setSettings(await invoke<Settings>("set_overlay_settings", { settings: { ...settings, ...change } }));
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  }

  return <>
    {error && <p role="alert">{error}</p>}
    {!settings && !error && <p role="status">Loading overlay settings…</p>}
    {settings && <>
      {([
        ["enabled", "In-game overlay"],
        ["gold", "Gold difference"],
        ["objectives", "Objective timers"],
        ["skills", "Skill level-up"],
      ] as const).map(([key, label]) => <div className="setting-row" key={key}>
        <div className="setting-copy"><label htmlFor={`overlay-${key}`}>{label}</label></div>
        <Switch id={`overlay-${key}`} checked={settings[key]} disabled={busy || (key !== "enabled" && !settings.enabled)} onCheckedChange={value => void update({ [key]: value })} />
      </div>)}
      <div className="setting-row">
        <div className="setting-copy"><label htmlFor="overlay-hotkey">Hold to show gold</label></div>
        <select id="overlay-hotkey" className="setting-select" value={settings.goldHotkey} disabled={busy || !settings.enabled || !settings.gold} onChange={event => void update({ goldHotkey: Number(event.target.value) })}>
          <option value={9}>Tab</option><option value={16}>Shift</option><option value={17}>Ctrl</option><option value={18}>Alt</option>
          {Array.from({ length: 12 }, (_, i) => <option key={i} value={112 + i}>F{i + 1}</option>)}
        </select>
      </div>
      <p>Use Borderless or Windowed in League's video settings. Exclusive Fullscreen is unsupported.</p>
      <p>Gold is an estimate of held items, excluding unspent gold and consumed or sold items. Skill hints use Swapper's latest cached max order for your champion, role and rank bracket; open a build in champion select first. Champions with unusual ability ranks are omitted.</p>
    </>}
  </>;
}
