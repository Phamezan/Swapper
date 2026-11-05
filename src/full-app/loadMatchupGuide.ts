import { invoke } from "@tauri-apps/api/core";
import type { MatchupGuideResult } from "./matchupGuide";

export async function loadMatchupGuide(championId: number, enemyChampionId: number, position: string): Promise<MatchupGuideResult | null> {
  const result = await invoke<MatchupGuideResult | null>("get_matchup_guide", { championId, enemyChampionId, position }).catch(() => null);
  if (import.meta.env.DEV && !result?.guide && result?.lookup !== "absent" && championId === 103 && enemyChampionId === 238 && position === "mid") {
    // Keep the import inside the dev guard so production bundles exclude fixture content.
    const { ahriVsZed } = await import("./fixtures/ahriVsZed");
    return { guide: ahriVsZed, record: result?.record ?? { wins: 5, losses: 20 }, lookup: "exact" };
  }
  if (!import.meta.env.DEV && result?.guide?.fixture) return null;
  return result;
}
