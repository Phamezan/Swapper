export type Metric = { average: number | null; delta: number | null; points: number[] };
export type Summary = { metrics: Metric[]; champions: { id: number; wins: number; losses: number; kda: number }[] };
export type Game = {
  id: number; queue: string; championId: number; createdAt: number; duration: number;
  win: boolean; kills: number; deaths: number; assists: number; metrics: (number | null)[];
};
export type Snapshot = {
  championNames: Record<string, string>;
  summoner: { riotId: string; level: number; icon: string | null };
  rank: { tier: string; division: string; lp: number; wins: number; losses: number } | null;
  fetchedAt: number; games: Game[]; summaries: Record<string, Summary>;
};
export type DashboardView = { snapshot: Snapshot | null; stale: boolean; message: string | null };
