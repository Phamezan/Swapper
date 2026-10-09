import { useEffect, useState } from "react";
import { cachedIcon, loadIcon } from "./iconLoader";

type Props = {
  id: number;
  name?: string;
  mode: "desktop" | "remote";
};

/** Champion portrait from the League client's game data: proxied on the phone,
 *  fetched over IPC in the desktop flyout. Shows a muted square until it loads. */
export function ChampionIcon({ id, name, mode }: Props) {
  const key = `champion:${id}`;
  const [src, setSrc] = useState<string | null>(() =>
    mode === "remote" && id > 0 ? `/api/champion/icon/${id}` : cachedIcon(key) ?? null,
  );

  useEffect(() => {
    if (id <= 0) {
      setSrc(null);
      return;
    }
    if (mode !== "desktop") {
      setSrc(`/api/champion/icon/${id}`);
      return;
    }
    const cached = cachedIcon(key);
    if (cached) {
      setSrc(cached);
      return;
    }
    let live = true;
    // Show the placeholder, not the previous champion, until this one loads.
    setSrc(null);
    void loadIcon(key, () =>
      import("@tauri-apps/api/core").then(({ invoke }) => invoke<string>("champion_icon", { id })),
    )
      .then((data) => {
        if (live) setSrc(data);
      })
      .catch(() => {
        if (live) setSrc(null);
      });
    return () => {
      live = false;
    };
  }, [id, key, mode]);

  if (!src) return <span className="champion-icon-fallback" aria-hidden title={name} />;
  return <img className="champion-icon" src={src} alt="" title={name} loading="lazy" draggable={false} />;
}
