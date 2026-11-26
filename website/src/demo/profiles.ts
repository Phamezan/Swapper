import { REALISTIC_ACCOUNTS, type RankData, type RealisticAccountConfig } from "./realisticData";

export type DemoProfile = RealisticAccountConfig;

export const DEMO_PROFILES: DemoProfile[] = REALISTIC_ACCOUNTS;
export const DEMO_ACCOUNTS = DEMO_PROFILES.map(p => p.account);

export type DemoSyncMessage =
  | { type: "swapper-demo:switched"; id: string; source: "tray" | "full" }
  | { type: "swapper-demo:sync"; id: string };

export function formatDemoRank(rank: RankData | null) {
  if (!rank) return "Unranked";
  const tier = rank.tier.charAt(0) + rank.tier.slice(1).toLowerCase();
  return `${tier}${rank.division ? ` ${rank.division}` : ""} · ${rank.lp} LP`;
}
