import { useEffect, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { Switch } from "@/components/ui/switch";

/** Settings → General row, shown in Fullsized App mode. */
export function LiveGameSettings() {
  const [enabled, setEnabled] = useState<boolean | null>(null);
  const [error, setError] = useState("");
  useEffect(() => {
    if (!isTauri()) return;
    let live = true;
    void invoke<boolean>("live_game_auto_open").then(value => { if (live) setEnabled(value); }).catch(reason => { if (live) setError(String(reason)); });
    return () => { live = false; };
  }, []);
  async function change(value: boolean) {
    setError("");
    try { await invoke("set_live_game_auto_open", { enabled: value }); setEnabled(value); }
    catch (reason) { setError(String(reason)); }
  }
  return <div className="setting-row">
    <div className="setting-copy">
      <label htmlFor="live-game-open-switch">Open live game on game start</label>
      <p>Opens minimized, without taking focus.</p>
      {error && <p role="alert">{error}</p>}
    </div>
    <Switch id="live-game-open-switch" checked={enabled ?? false} disabled={enabled === null} onCheckedChange={value => void change(value)} aria-label="Open live game on game start" />
  </div>;
}
