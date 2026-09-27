import { useEffect, useState } from "react";

const cache = new Map<number, string>();

type Props = {
  id: number;
  mode: "desktop" | "remote";
  className?: string;
};

/** Rune art from the League client's game data: proxied on the phone, fetched
 *  over IPC in the desktop flyout, because the webview cannot reach the LCU. */
export function RuneIcon({ id, mode, className }: Props) {
  const [src, setSrc] = useState<string | null>(() =>
    mode === "remote" ? `/api/rune/icon/${id}` : cache.get(id) ?? null,
  );

  useEffect(() => {
    if (mode !== "desktop") {
      setSrc(`/api/rune/icon/${id}`);
      return;
    }
    const cached = cache.get(id);
    if (cached) {
      setSrc(cached);
      return;
    }
    let live = true;
    // The desktop webview cannot reach the LCU, so icons come over IPC. The
    // phone uses the plain URL above and never loads the Tauri API.
    void import("@tauri-apps/api/core")
      .then(({ invoke }) => invoke<string>("rune_icon", { id }))
      .then((data) => {
        cache.set(id, data);
        if (live) setSrc(data);
      })
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [id, mode]);

  if (!src) return <span className={`rune-icon-fallback ${className ?? ""}`} aria-hidden />;
  return <img className={className} src={src} alt="" loading="lazy" draggable={false} />;
}
