import type { LaunchTarget } from "./types";

export const LAUNCH_TARGET_NAMES: Record<LaunchTarget, string> = { riot_client: "Riot Client", league: "League", valorant: "VALORANT" };

/** What a switch opens: the account's override, else the global setting (League when unknown). */
export const resolveLaunchTarget = (override?: LaunchTarget | null, global?: LaunchTarget | null): LaunchTarget =>
  override ?? global ?? "league";

/** The per-account value to store for a pick: `null` when it matches the global setting, so it keeps following Settings. */
export const launchOverride = (pick: LaunchTarget, global?: LaunchTarget | null): LaunchTarget | null =>
  pick === resolveLaunchTarget(null, global) ? null : pick;

/** "Open …" for what a switch opens. */
export const openLabel = (override?: LaunchTarget | null, global?: LaunchTarget | null) =>
  `Open ${LAUNCH_TARGET_NAMES[resolveLaunchTarget(override, global)]}`;
