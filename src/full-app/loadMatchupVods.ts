import type { MatchupVod } from "./matchupGuide";

export async function loadMatchupVods(champion: number, enemy: number, lane: string): Promise<MatchupVod[]> {
  if (import.meta.env.DEV && champion === 103 && enemy === 238 && lane === "mid") {
    const { ahriVsZedVods } = await import("./fixtures/ahriVsZedVods");
    return ahriVsZedVods;
  }
  return [];
}
