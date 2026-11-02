import type { MatchupVod } from "./matchupGuide";
import { invoke } from '@tauri-apps/api/core';

export async function loadMatchupVods(champion: number, enemy: number, lane: string): Promise<MatchupVod[]> {
  const vods = await invoke<MatchupVod[]>('get_matchup_vods', { championId: champion, enemyChampionId: enemy, position: lane }).catch(() => []);
  if (vods?.length) return vods;
  if (import.meta.env.DEV && champion === 103 && enemy === 238 && lane === "mid") {
    const { ahriVsZedVods } = await import("./fixtures/ahriVsZedVods");
    return ahriVsZedVods;
  }
  return [];
}
