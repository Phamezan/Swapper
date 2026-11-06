import { fmtAgo } from "./types";
import { Tooltip } from "../full-app/ui/Tooltip";

type Props = {
  /** Whether the panel is showing the last successful result. */
  stale: boolean;
  /** Unix milliseconds of that last successful fetch, when known. */
  updatedAt: number | null;
  /** An optional extra class for placement in a particular panel. */
  className?: string;
};

/**
 * A subtle "Cached · updated X ago" badge, shown only when a panel is showing
 * the last successful result because the live provider failed. It keeps the
 * data visible while making its age honest.
 */
export function CachedBadge({ stale, updatedAt, className }: Props) {
  if (!stale) return null;
  return (
    <Tooltip label="The live source could not be reached; showing the last successful result.">
      <span className={`cached-badge${className ? ` ${className}` : ""}`}>
        Cached · updated {fmtAgo(updatedAt)}
      </span>
    </Tooltip>
  );
}
