import { useEffect, useState } from "react";
import { cachedIcon, loadIcon } from "../runes/iconLoader";
import { isPlaceholderPhoto } from "./matchupVodModel";

// Cached like a photo so a silhouette is measured once per session, never shown.
const NO_PHOTO = "none";

async function realPhoto(data: string): Promise<string> {
  const image = new Image();
  image.src = data;
  await image.decode();
  const canvas = document.createElement("canvas");
  canvas.width = image.naturalWidth;
  canvas.height = image.naturalHeight;
  const context = canvas.getContext("2d");
  if (!context) return data;
  context.drawImage(image, 0, 0);
  return isPlaceholderPhoto(context.getImageData(0, 0, canvas.width, canvas.height).data) ? NO_PHOTO : data;
}

/** A pro's headshot as a data URL; null while loading, without a name, or when the CDN has none or only a silhouette. */
export function usePlayerPhoto(name: string | null): string | null {
  const key = `player:${name}`;
  const [photo, setPhoto] = useState<string | null>(() => (name ? cachedIcon(key) ?? null : null));
  useEffect(() => {
    if (!name) { setPhoto(null); return; }
    let live = true;
    // The desktop webview cannot reach the CDN directly, so the PNG comes over IPC like team logos.
    void loadIcon(key, () => import("@tauri-apps/api/core").then(({ invoke }) => invoke<string>("player_photo", { player: name })).then(realPhoto))
      .then(data => { if (live) setPhoto(data); })
      .catch(() => { if (live) setPhoto(null); });
    return () => { live = false; };
  }, [name, key]);
  return photo === NO_PHOTO ? null : photo;
}
