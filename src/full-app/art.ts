import { useEffect, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { cachedIcon, loadIcon } from "../runes/iconLoader";

/** Champion splash ("splash", ~100 KB) or square tile ("tile", ~40 KB), disk
 *  cached by the backend. Splashes are also shared in memory through the icon loader. */
export function useChampionArt(id: number | null | undefined, kind: "splash" | "tile"): string | null {
  const key = `art:${kind}:${id}`;
  const [src, setSrc] = useState<string | null>(() => (id ? cachedIcon(key) ?? null : null));
  useEffect(() => {
    if (!id || !isTauri()) { setSrc(null); return; }
    const cached = cachedIcon(key);
    if (cached) { setSrc(cached); return; }
    let live = true;
    setSrc(null);
    // Tiles are many (a matchup strip has 150+): read them from the disk cache
    // each time instead of keeping every data URL alive in the shared map.
    const load = kind === "tile" ? invoke<string>("champion_art", { id, kind }) : loadIcon(key, () => invoke<string>("champion_art", { id, kind }));
    void load
      .then(data => { if (live) setSrc(data); })
      .catch(() => { if (live) setSrc(null); });
    return () => { live = false; };
  }, [id, kind, key]);
  return src;
}
