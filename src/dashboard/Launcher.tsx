import { useEffect, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Switch } from "@/components/ui/switch";

/** Only these small controls are imported by the flyout; dashboard pages have their own entry. */
export function DashboardLauncher({ setting = false }: { setting?: boolean }) {
  const [enabled, setEnabled] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  useEffect(() => {
    if (!isTauri()) return;
    let live = true;
    const stop = listen<boolean>("dashboard_enabled", ({ payload }) => { if (live) setEnabled(payload); });
    void invoke<boolean>("dashboard_enabled").then(value => { if (live) setEnabled(value); }).catch(reason => { if (live) setError(String(reason)); });
    return () => { live = false; void stop.then(unlisten => unlisten()); };
  }, []);
  async function change(value: boolean) {
    setBusy(true); setError("");
    try { await invoke("set_dashboard_enabled", { enabled: value }); setEnabled(value); }
    catch (reason) { setError(String(reason)); }
    finally { setBusy(false); }
  }
  if (!setting && !enabled) return null;
  return <div className={setting ? "setting-row" : "dashboard-launch"} style={setting ? undefined : { padding: "8px 18px", borderBottom: "1px solid var(--line-subtle)" }}>
    {setting ? <><div className="setting-copy"><label htmlFor="dashboard-switch">Dashboard window</label><p>Open a separate window for recent games, builds and matchups.</p>{error && <p role="alert">{error}</p>}</div>
      <Switch id="dashboard-switch" checked={enabled} disabled={busy || !isTauri()} onCheckedChange={value => void change(value)} aria-label="Dashboard window" /></>
      : <><button type="button" onClick={() => { setError(""); void invoke("open_dashboard").catch(reason => setError(String(reason))); }}>Open dashboard ↗</button>{error && <p role="alert">{error}</p>}</>}
  </div>;
}
