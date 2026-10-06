import { useEffect, useState, type ReactNode } from "react";
import { cachedIcon, loadIcon } from "./iconLoader";

/** League positions, as the slugs used across the rune and pro-build data. */
export type RoleKey = "all" | "top" | "jungle" | "mid" | "adc" | "support" | "none";

/** Maps an LCU `assignedPosition`, an op.gg position or a display label to a
 *  role key. Unknown values fall back to `none`. */
export function roleKey(role: string): RoleKey {
  switch (role.trim().toLowerCase()) {
    case "top":
      return "top";
    case "jungle":
    case "jg":
      return "jungle";
    case "mid":
    case "middle":
      return "mid";
    case "adc":
    case "bottom":
    case "bot":
      return "adc";
    case "support":
    case "supp":
    case "utility":
      return "support";
    case "all":
      return "all";
    default:
      return "none";
  }
}

/** Role key -> the client's static-asset SVG name. Only these five are proxied. */
const ASSET: Partial<Record<RoleKey, string>> = {
  top: "top",
  jungle: "jungle",
  mid: "middle",
  adc: "bottom",
  support: "utility",
};

/** How many times a desktop icon load is attempted before giving up. The fetch
 *  can fail transiently (the LCU does not serve the asset in-game and the CDragon
 *  mirror is sometimes down), so a late icon beats a blank one. Failed loads are
 *  never cached, so each retry starts a fresh request. */
const ICON_LOAD_TRIES = 3;
/** Wait between retries. */
const ICON_RETRY_MS = 3_000;

/** The two roles that have no client asset keep a small inline glyph. */
const PATHS: Partial<Record<RoleKey, ReactNode>> = {
  // All: a grid of every role.
  all: (
    <>
      <rect x="4" y="4" width="7" height="7" rx="1.5" />
      <rect x="13" y="4" width="7" height="7" rx="1.5" />
      <rect x="4" y="13" width="7" height="7" rx="1.5" />
      <rect x="13" y="13" width="7" height="7" rx="1.5" />
    </>
  ),
  none: <circle cx="12" cy="12" r="3" />,
};

/** An inline League-position glyph, used by the pick filters and next to the
 *  role on rune and pro-build cards.
 *
 *  The five real positions use the League client's own SVG, fetched through the
 *  same proxy as the rune icons and rendered as a CSS mask so the current text
 *  colour controls it: dim grey when idle, gold when the parent marks it
 *  active. */
export function RoleIcon({
  role,
  size = 18,
  className,
  mode = "remote",
}: {
  role: string;
  size?: number;
  className?: string;
  mode?: "desktop" | "remote";
}) {
  const key = roleKey(role);
  const asset = ASSET[key] ?? null;
  const cacheKey = `role:${asset}`;
  const [src, setSrc] = useState<string | null>(() =>
    asset ? (mode === "remote" ? `/api/role/icon/${asset}` : cachedIcon(cacheKey) ?? null) : null,
  );

  useEffect(() => {
    if (!asset) return;
    if (mode !== "desktop") {
      setSrc(`/api/role/icon/${asset}`);
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
        import("@tauri-apps/api/core").then(({ invoke }) => invoke<string>("role_icon", { role: asset })),
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
  }, [asset, cacheKey, mode]);

  if (!asset) {
    return (
      <svg
        width={size}
        height={size}
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth={1.9}
        strokeLinecap="round"
        strokeLinejoin="round"
        className={className}
        aria-hidden
      >
        {PATHS[key]}
      </svg>
    );
  }

  if (!src) {
    return (
      <span
        className={`role-icon role-icon-fallback ${className ?? ""}`}
        style={{ width: size, height: size }}
        aria-hidden
      />
    );
  }

  const url = `url("${src}")`;
  return (
    <span
      className={`role-icon ${className ?? ""}`}
      aria-hidden
      style={{
        width: size,
        height: size,
        WebkitMaskImage: url,
        maskImage: url,
        WebkitMaskRepeat: "no-repeat",
        maskRepeat: "no-repeat",
        WebkitMaskSize: "contain",
        maskSize: "contain",
        WebkitMaskPosition: "center",
        maskPosition: "center",
      }}
    />
  );
}
