import { useSettingsVersion } from "../settingsEvents";
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Switch } from "../components/ui/switch";
import { Picker } from "./Picker";
import { Skeleton } from "./ui/States";

type Settings = { enabled: boolean; gold: boolean; objectives: boolean; skills: boolean; goldHotkey: number };
const widgets = [
  { key: "gold", title: "Gold difference", description: "Held-item value while you hold the key." },
  { key: "objectives", title: "Objective timers", description: "Upcoming neutral objectives." },
  { key: "skills", title: "Skill level-up", description: "Next ability to level." },
] as const;
/** The in-game level-up hint as the overlay draws it (render.rs `arrow`, unit 40): a graphite box with a green
 *  double chevron, three green diamonds above in a rising wave (one frozen frame here). */
function SkillHint() {
  const chevron = (top: number) => `48,${top + 8} 60,${top} 72,${top + 8}`;
  return <svg className="fa-skill-hint" viewBox="0 0 120 120" role="img" aria-label="Skill level-up hint: a graphite box with a green double chevron and three diamonds above it">
    <rect x="34" y="52" width="52" height="52" rx="4.8" fill="#181a17" fillOpacity=".95" stroke="#c5e39e" strokeWidth="3.2" />
    {[69.6, 78.4].map(top => <polyline key={top} points={chevron(top)} fill="none" stroke="#c5e39e" strokeWidth="4.4" strokeLinecap="round" strokeLinejoin="round" />)}
    {[[40.8, 1], [28.8, .6], [16.8, .3]].map(([y, alpha]) => <path key={y} d={`M60 ${y - 4}L64 ${y}L60 ${y + 4}L56 ${y}Z`} fill="#c5e39e" fillOpacity={alpha} />)}
  </svg>;
}

export function OverlayCards() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const settingsVersion = useSettingsVersion();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  useEffect(() => {
    let live = true;
    void invoke<Settings>("get_overlay_settings").then(value => { if (live) setSettings(value); }).catch(reason => { if (live) setError(String(reason)); });
    return () => { live = false; };
  }, [settingsVersion]);
  async function update(change: Partial<Settings>) {
    if (!settings || busy) return;
    setBusy(true); setError("");
    try { setSettings(await invoke<Settings>("set_overlay_settings", { settings: { ...settings, ...change } })); }
    catch (reason) { setError(String(reason)); }
    finally { setBusy(false); }
  }
  return <div className="fa-overlay-settings">
    {error && <p role="alert">{error}</p>}
    {!settings ? <Skeleton label="Loading overlay settings" rows={3} height={64} /> : <>
      <div className="setting-row"><div className="setting-copy"><strong>In-game overlay</strong><p>Needs Borderless or Windowed mode.</p></div><Switch aria-label="In-game overlay" checked={settings.enabled} disabled={busy} onCheckedChange={enabled => void update({ enabled })} /></div>
      <div className="fa-overlay-grid">{widgets.map(widget => <article className="fa-overlay-card" key={widget.key}>
        <div className={`fa-overlay-preview is-${widget.key}`}>{widget.key === "skills" ? <SkillHint /> : <img src="/overlay-fixture.png" alt={`${widget.title} preview from Swapper's native overlay fixture`} loading="lazy" />}<small>Preview</small></div>
        <div className="fa-overlay-copy"><div><h3>{widget.title}</h3><Switch aria-label={widget.title} checked={settings[widget.key]} disabled={busy || !settings.enabled} onCheckedChange={value => void update({ [widget.key]: value })} /></div><p>{widget.description}</p></div>
      </article>)}</div>
      <div className="setting-row"><div className="setting-copy"><strong>Hold to show gold</strong><p>Excludes unspent gold and consumed or sold items.</p></div><Picker label="Overlay hotkey" value={String(settings.goldHotkey)} disabled={busy || !settings.enabled || !settings.gold} onChange={value => void update({ goldHotkey: Number(value) })} options={[[9, "Tab"], [16, "Shift"], [17, "Ctrl"], [18, "Alt"], ...Array.from({ length: 12 }, (_, i) => [112 + i, `F${i + 1}`])].map(([value, label]) => ({ value: String(value), label: String(label) }))} /></div>
      <p className="fa-data-note">Skill hints need a cached build for your champion, role and rank. Champions with unusual ability ranks are omitted.</p>
    </>}
  </div>;
}
