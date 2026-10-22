import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type IconPixels = { width: number; height: number; rgba: string };

// One lookup per app session: the icon comes from the installed Riot Client, never from Swapper.
let cached: Promise<string | null> | null = null;

function load(): Promise<string | null> {
  cached ??= invoke<IconPixels | null>("riot_client_icon").then(icon => {
    if (!icon) return null;
    const bytes = Uint8ClampedArray.from(atob(icon.rgba), c => c.charCodeAt(0));
    const canvas = document.createElement("canvas");
    canvas.width = icon.width;
    canvas.height = icon.height;
    canvas.getContext("2d")?.putImageData(new ImageData(bytes, icon.width, icon.height), 0, 0);
    return canvas.toDataURL("image/png");
  }).catch(() => null);
  return cached;
}

/** The installed Riot Client's icon, or nothing when it can't be found. */
export function RiotClientIcon({ size = 18 }: { size?: number }) {
  const url = useRiotClientIcon();
  return url ? <img className="riot-client-icon" src={url} width={size} height={size} alt="" /> : null;
}

/** The installed Riot Client's icon as a data URL, or null when it can't be found. */
export function useRiotClientIcon(): string | null {
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    void load().then(found => { if (live) setUrl(found); });
    return () => { live = false; };
  }, []);
  return url;
}
