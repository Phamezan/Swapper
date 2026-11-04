import type { MatchupVod } from "./matchupGuide";
import { invoke } from '@tauri-apps/api/core';

export async function loadMatchupVods(champion: number, enemy: number, lane: string): Promise<MatchupVod[] | null> {
  const vods = await invoke<MatchupVod[]>('get_matchup_vods', { championId: champion, enemyChampionId: enemy, position: lane }).catch(() => null);
  if (vods === null) return null;
  if (vods?.length) return vods;
  if (import.meta.env.DEV && champion === 103 && enemy === 238 && lane === "mid") {
    const { ahriChampionVods } = await import("./fixtures/ahriChampionVods");
    return ahriChampionVods.filter(vod => vod.enemy === 238 && vod.lane === "mid");
  }
  return [];
}

/** Every VOD of one champion, each tagged with its enemy and lane. */
export async function loadChampionVods(champion: number): Promise<MatchupVod[] | null> {
  const vods = await invoke<MatchupVod[]>('get_champion_vods', { championId: champion }).catch(() => null);
  if (vods === null) return null;
  if (vods?.length) return vods;
  if (import.meta.env.DEV && champion === 103) return (await import("./fixtures/ahriChampionVods")).ahriChampionVods;
  return [];
}


/** Every indexed VOD, with the played champion and matchup attached. */
export async function loadAllVods(): Promise<MatchupVod[] | null> {
  const vods = await invoke<MatchupVod[]>('get_all_vods').catch(() => null);
  if (vods === null) return null;
  if (vods?.length) return vods;
  if (import.meta.env.DEV) return (await import("./fixtures/ahriChampionVods")).ahriChampionVods.map(vod => ({ ...vod, champion: 103 }));
  return [];
}
