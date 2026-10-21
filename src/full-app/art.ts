import { useEffect, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { cachedIcon, loadIcon } from "../runes/iconLoader";

/** Champion splash ("splash", ~100 KB) or square tile ("tile", ~40 KB), disk
 *  cached by the backend and shared in memory through the icon loader. */
export function useChampionArt(id: number | null | undefined, kind: "splash" | "tile"): string | null {
  const key = `art:${kind}:${id}`;
  const [src, setSrc] = useState<string | null>(() => (id ? cachedIcon(key) ?? null : null));
  useEffect(() => {
    if (!id || !isTauri()) { setSrc(null); return; }
    const cached = cachedIcon(key);
    if (cached) { setSrc(cached); return; }
    let live = true;
    setSrc(null);
    void loadIcon(key, () => invoke<string>("champion_art", { id, kind }))
      .then(data => { if (live) setSrc(data); })
      .catch(() => { if (live) setSrc(null); });
    return () => { live = false; };
  }, [id, kind, key]);
  return src;
}
