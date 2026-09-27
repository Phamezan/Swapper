import { useEffect, useState } from "react";
import { cachedIcon, loadIcon } from "./iconLoader";

type Props = {
  id: number;
  mode: "desktop" | "remote";
  className?: string;
};

/** Summoner-spell art from the League client's game data: proxied on the
 *  phone, fetched over IPC in the desktop flyout, like the rune icons. */
export function SpellIcon({ id, mode, className }: Props) {
  const key = `spell:${id}`;
  const [src, setSrc] = useState<string | null>(() =>
    mode === "remote" && id > 0 ? `/api/spell/icon/${id}` : cachedIcon(key) ?? null,
  );

  useEffect(() => {
    if (id <= 0) {
      setSrc(null);
      return;
    }
    if (mode !== "desktop") {
      setSrc(`/api/spell/icon/${id}`);
      return;
    }
    const cached = cachedIcon(key);
    if (cached) {
      setSrc(cached);
      return;
    }
    let live = true;
    void loadIcon(key, () =>
      import("@tauri-apps/api/core").then(({ invoke }) => invoke<string>("spell_icon", { id })),
    )
      .then((data) => {
        if (live) setSrc(data);
      })
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [id, key, mode]);

  if (id <= 0 || !src) return <span className={`spell-icon-fallback ${className ?? ""}`} aria-hidden />;
  return <img className={`spell-icon ${className ?? ""}`} src={src} alt="" loading="lazy" draggable={false} />;
}
