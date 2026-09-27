import { useEffect, useState } from "react";
import { cachedIcon, loadIcon } from "./iconLoader";

type Props = {
  id: number;
  mode: "desktop" | "remote";
  className?: string;
};

/** Rune art from the League client's game data: proxied on the phone, fetched
 *  over IPC in the desktop flyout, because the webview cannot reach the LCU. */
export function RuneIcon({ id, mode, className }: Props) {
  const key = `rune:${id}`;
  const [src, setSrc] = useState<string | null>(() =>
    mode === "remote" ? `/api/rune/icon/${id}` : cachedIcon(key) ?? null,
  );

  useEffect(() => {
    if (mode !== "desktop") {
      setSrc(`/api/rune/icon/${id}`);
      return;
    }
    const cached = cachedIcon(key);
    if (cached) {
      setSrc(cached);
      return;
    }
    let live = true;
    // The desktop webview cannot reach the LCU, so icons come over IPC. The
    // phone uses the plain URL above and never loads the Tauri API.
    void loadIcon(key, () =>
      import("@tauri-apps/api/core").then(({ invoke }) => invoke<string>("rune_icon", { id })),
    )
      .then((data) => {
        if (live) setSrc(data);
      })
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [id, key, mode]);

  if (!src) return <span className={`rune-icon-fallback ${className ?? ""}`} aria-hidden />;
  return <img className={className} src={src} alt="" loading="lazy" draggable={false} />;
}
