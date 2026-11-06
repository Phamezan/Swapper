import { useEffect, useState } from "react";
import { cachedIcon, loadIcon } from "./iconLoader";
import { Tooltip } from "../full-app/ui/Tooltip";

type Props = {
  id: number;
  name?: string;
  mode: "desktop" | "remote";
};

/** Champion portrait from the League client's game data: proxied on the phone,
 *  fetched over IPC in the desktop flyout. Shows a muted square until it loads. */
export function ChampionIcon({ id, name, mode }: Props) {
  const key = `champion:${id}`;
  const unknownIcon = "https://raw.communitydragon.org/latest/plugins/rcp-be-lol-game-data/global/default/v1/champion-icons/-1.png";
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

  if (id === -1) return <img className="champion-icon" src={unknownIcon} alt={name ?? ""} loading="lazy" draggable={false} />;
  if (!src) return <Tooltip label={name}><span className="champion-icon-fallback" role={name ? "img" : undefined} aria-label={name} aria-hidden={!name} /></Tooltip>;
  return <Tooltip label={name}><img className="champion-icon" src={src} alt={name ?? ""} loading="lazy" draggable={false} /></Tooltip>;
}
