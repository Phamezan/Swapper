import type { ReactNode } from "react";

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

const PATHS: Record<RoleKey, ReactNode> = {
  // Top: the lane's mountain.
  top: (
    <>
      <path d="M4 19h16" />
      <path d="M12 5l7 14H5z" />
    </>
  ),
  // Jungle: a tree.
  jungle: (
    <>
      <path d="M12 3 6 12h4l-3 5h10l-3-5h4z" />
      <path d="M12 17v4" />
    </>
  ),
  // Mid: a crosshair.
  mid: (
    <>
      <circle cx="12" cy="12" r="6" />
      <path d="M12 2v4M12 18v4M2 12h4M18 12h4" />
    </>
  ),
  // Bot: the lane's arrow, pointing down.
  adc: (
    <>
      <path d="M4 5h16" />
      <path d="M12 7v12" />
      <path d="M8 15l4 4 4-4" />
    </>
  ),
  // Support: a shield.
  support: <path d="M12 3l7 3v6c0 4-3 7-7 9-4-2-7-5-7-9V6z" />,
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
 *  role on rune and pro-build cards. */
export function RoleIcon({
  role,
  size = 18,
  className,
}: {
  role: string;
  size?: number;
  className?: string;
}) {
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
      {PATHS[roleKey(role)]}
    </svg>
  );
}
