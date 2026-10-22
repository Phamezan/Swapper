import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

// League's own match-history icons (served by the `objective_icon` command; side-tinted where the client has both).
const FILES: Record<string, [string, string]> = {
  Dragons: ["dragon-100", "dragon-200"], Barons: ["baron-100", "baron-200"], Heralds: ["herald-100", "herald-200"],
  Towers: ["tower-100", "tower-200"], Inhibitors: ["inhibitor-100", "inhibitor-200"], Grubs: ["grub", "grub"],
};
const loaded = new Map<string, Promise<string | null>>();
const load = (name: string) => {
  if (!loaded.has(name)) loaded.set(name, invoke<string>("objective_icon", { name }).catch(() => { loaded.delete(name); return null; }));
  return loaded.get(name)!;
};

export function ObjectiveIcon({ label, side }: { label: string; side: number }) {
  const name = FILES[label]?.[side === 200 ? 1 : 0];
  const [src, setSrc] = useState<string | null>(null);
  useEffect(() => {
    if (!name) return;
    let live = true;
    void load(name).then(url => { if (live) setSrc(url); });
    return () => { live = false; };
  }, [name]);
  return src ? <img className="md-objective-icon" src={src} alt="" /> : <span className="md-objective-icon is-text">{label.slice(0, 1)}</span>;
}
