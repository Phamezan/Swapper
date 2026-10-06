import { useEffect, useState } from "react";
import { cachedIcon, loadIcon } from "./iconLoader";

/** op.gg tier slug -> the client's crest asset name. Kept in step with
 *  `runes::ranks::asset_name`. */
const ASSET: Record<string, string> = {
  all: "unranked",
  gold_plus: "gold",
  platinum_plus: "platinum",
  emerald_plus: "emerald",
  diamond_plus: "diamond",
  master_plus: "master",
  challenger: "challenger",
};

/** How many times a desktop icon load is attempted before giving up. The fetch
 *  can fail transiently (the LCU does not serve the asset in-game and the CDragon
 *  mirror is sometimes down), so a late crest beats a blank one. Failed loads are
 *  never cached, so each retry starts a fresh request. */
const ICON_LOAD_TRIES = 3;
/** Wait between retries. */
const ICON_RETRY_MS = 3_000;

type Props = {
  /** The op.gg bracket slug, e.g. `emerald_plus`. */
  tier: string;
  size?: number;
  mode?: "desktop" | "remote";
  className?: string;
};

/** A League ranked mini-crest: proxied on the phone, fetched over IPC in the
 *  desktop flyout, like the role icons. Unknown brackets show a neutral dot. */
export function RankCrest({ tier, size = 22, mode = "remote", className }: Props) {
  const asset = ASSET[tier] ?? null;
  const cacheKey = `rank:${tier}`;
  const [src, setSrc] = useState<string | null>(() =>
    asset ? (mode === "remote" ? `/api/rank/icon/${tier}` : cachedIcon(cacheKey) ?? null) : null,
  );

  useEffect(() => {
    if (!asset) {
      setSrc(null);
      return;
    }
    if (mode !== "desktop") {
      setSrc(`/api/rank/icon/${tier}`);
      return;
    }
    const cached = cachedIcon(cacheKey);
    if (cached) {
      setSrc(cached);
      return;
    }
    let live = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    // The desktop webview cannot reach the LCU, so the SVG comes over IPC.
    const attempt = (triesLeft: number) => {
      void loadIcon(cacheKey, () =>
        import("@tauri-apps/api/core").then(({ invoke }) => invoke<string>("rank_icon", { tier })),
      )
        .then((data) => {
          if (live) setSrc(data);
        })
        .catch(() => {
          if (live && triesLeft > 1) {
            timer = setTimeout(() => attempt(triesLeft - 1), ICON_RETRY_MS);
          }
        });
    };
    attempt(ICON_LOAD_TRIES);
    return () => {
      live = false;
      if (timer) clearTimeout(timer);
    };
  }, [asset, tier, cacheKey, mode]);

  if (!asset || !src) {
    return (
      <span
        className={`rank-crest-fallback ${className ?? ""}`}
        style={{ width: size, height: size }}
        aria-hidden
      />
    );
  }
  return (
    <img
      className={`rank-crest ${className ?? ""}`}
      src={src}
      alt=""
      width={size}
      height={size}
      draggable={false}
    />
  );
}
