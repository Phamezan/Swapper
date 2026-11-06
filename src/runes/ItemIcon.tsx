import { useEffect, useState } from "react";
import { cachedIcon, loadIcon } from "./iconLoader";
import { Tooltip } from "../full-app/ui/Tooltip";

type Props = {
  id: number;
  /** The item's display name; used as the image title and alt text. */
  name?: string;
  mode: "desktop" | "remote";
  className?: string;
};

/** Item art from the League client's game data: proxied on the phone, fetched
 *  over IPC in the desktop flyout, like the rune and spell icons. The name is
 *  shown on hover (title) and to screen readers. */
export function ItemIcon({ id, name, mode, className }: Props) {
  const key = `item:${id}`;
  const [src, setSrc] = useState<string | null>(() =>
    mode === "remote" && id > 0 ? `/api/item/icon/${id}` : cachedIcon(key) ?? null,
  );

  useEffect(() => {
    if (id <= 0) {
      setSrc(null);
      return;
    }
    if (mode !== "desktop") {
      setSrc(`/api/item/icon/${id}`);
      return;
    }
    const cached = cachedIcon(key);
    if (cached) {
      setSrc(cached);
      return;
    }
    let live = true;
    void loadIcon(key, () =>
      import("@tauri-apps/api/core").then(({ invoke }) => invoke<string>("item_icon", { id })),
    )
      .then((data) => {
        if (live) setSrc(data);
      })
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [id, key, mode]);

  const label = name || `Item ${id}`;
  if (id <= 0) return <span className={`item-icon-fallback ${className ?? ""}`} aria-hidden />;
  if (!src) {
    return (
      <Tooltip label={label}>
        <span
          className={`item-icon-fallback ${className ?? ""}`}
          role="img" aria-label={label}
        />
      </Tooltip>
    );
  }
  return (
    <Tooltip label={label}>
      <img
        className={`item-icon ${className ?? ""}`}
        src={src}
        alt={label}
        loading="lazy"
        draggable={false}
      />
    </Tooltip>
  );
}
