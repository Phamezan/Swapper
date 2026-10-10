import { useEffect, useState } from "react";
import { cachedIcon, loadIcon } from "./iconLoader";

/** How many times a desktop logo load is attempted before giving up. The CDN
 *  can fail transiently, so a late logo beats a permanent initial. Failed loads
 *  are never cached, so each retry starts a fresh request. */
const ICON_LOAD_TRIES = 3;
/** Wait between retries. */
const ICON_RETRY_MS = 3_000;

/** A short label for a team: the first letter of up to two words. */
function initials(team: string): string {
  const words = team.trim().split(/\s+/).filter(Boolean);
  return words
    .slice(0, 2)
    .map((word) => Array.from(word)[0] ?? "")
    .join("")
    .toUpperCase();
}

type Props = {
  /** The team's display name, e.g. "Karmine Corp". */
  team: string;
  size?: number;
  mode?: "desktop" | "remote";
  className?: string;
};

/** A pro team's logo: proxied on the phone, fetched over IPC in the desktop
 *  flyout, like the role and rank icons. When the logo is missing the square
 *  shows the team's initials, never a broken image. */
export function TeamLogo({ team, size = 24, mode = "remote", className }: Props) {
  const cacheKey = `team:${team}`;
  const [src, setSrc] = useState<string | null>(() =>
    team
      ? mode === "remote"
        ? `/api/team/icon/${encodeURIComponent(team)}`
        : cachedIcon(cacheKey) ?? null
      : null,
  );

  useEffect(() => {
    if (!team) {
      setSrc(null);
      return;
    }
    if (mode !== "desktop") {
      setSrc(`/api/team/icon/${encodeURIComponent(team)}`);
      return;
    }
    const cached = cachedIcon(cacheKey);
    if (cached) {
      setSrc(cached);
      return;
    }
    let live = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    // The desktop webview cannot reach the CDN directly, so the PNG comes over IPC.
    const attempt = (triesLeft: number) => {
      void loadIcon(cacheKey, () =>
        import("@tauri-apps/api/core").then(({ invoke }) => invoke<string>("team_icon", { team })),
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
  }, [team, cacheKey, mode]);

  if (src) {
    return (
      <img
        className={`team-logo ${className ?? ""}`}
        src={src}
        alt=""
        width={size}
        height={size}
        draggable={false}
      />
    );
  }
  return (
    <span
      className={`team-logo team-logo-fallback ${className ?? ""}`}
      style={{ width: size, height: size, fontSize: Math.round(size * 0.4) }}
      aria-hidden
    >
      {initials(team)}
    </span>
  );
}
