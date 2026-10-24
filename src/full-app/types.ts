export type Metric = { average: number | null; delta: number | null; points: number[] };
export type RoleRecord = { role: string; wins: number; games: number };
export type Summary = {
  metrics: Metric[];
  champions: { id: number; wins: number; losses: number; kda: number }[];
  roles: RoleRecord[];
};
export type Game = {
  id: number; queue: string; championId: number; createdAt: number; duration: number;
  win: boolean; kills: number; deaths: number; assists: number; metrics: (number | null)[];
  /** Empty for games cached before these fields existed. */
  items: number[]; spells: number[]; keystone: number; cs: number; level: number;
  role: string; placement: number;
  /** Solo/Duo LP change; only when Swapper recorded the rank around the game. */
  lpChange?: number | null;
  /** All ten players, own team first; empty for games cached before this field. */
  players: Player[];
  /** Index into `players` of the lane opponent, when unambiguous. */
  opponent?: number | null;
};
export type Player = {
  championId: number; riotId: string; me: boolean; ally: boolean; role: string;
  kills: number; deaths: number; assists: number; cs: number; damage: number; gold: number; vision: number; items: number[];
};
/** Per-game averages of a champion in a role and tier (lolalytics). */
export type Bench = { kills: number; deaths: number; assists: number; damage: number; gold: number; cs: number; games: number };
export type Snapshot = {
  championNames: Record<string, string>;
  summoner: { riotId: string; level: number; icon: string | null };
  rank: { tier: string; division: string; lp: number; wins: number; losses: number } | null;
  fetchedAt: number; games: Game[]; summaries: Record<string, Summary>;
  /** Solo/Duo rank points Swapper recorded, oldest first; absent in older caches. */
  lpHistory?: LpPoint[];
  /** Average current rank of recent Solo/Duo lobbies: Swapper's own MMR estimate. */
  mmr?: Mmr | null;
};
/** `trend`: ladder LP the newer half of the lobbies sits above (negative: below) the older half. */
export type Mmr = { tier: string; division: string; lp: number; games: number; trend: number | null };
export type LpPoint = { at: number; tier: string; division: string; lp: number; wins: number; losses: number;
  /** Set ("opgg") on points seeded from another tracker; those carry no win/loss counts. */
  source?: string };
/** Last saved rank and recent record of a saved account (key: account id). */
export type AccountCard = { rank: Snapshot["rank"]; wins: number; losses: number; lastPlayed: number; updatedAt: number };
export type DashboardView = { snapshot: Snapshot | null; cards: Record<string, AccountCard>; stale: boolean; message: string | null };
/** One blind pick from the `recommended_picks` command, best first. */
export type RecommendedPick = { championId: number; winRate: number; games: number; flex: boolean };

export type MatchDetailPlayer = {
  participantId: number; teamId: number; championId: number; riotId: string; me: boolean; role: string;
  level: number | null; spells: number[]; perks: number[]; keystone: number | null; secondaryStyle: number | null;
  kills: number | null; deaths: number | null; assists: number | null; cs: number | null;
  damage: number | null; gold: number | null; vision: number | null; damageTaken: number | null;
  wardsPlaced: number | null; wardsKilled: number | null; items: number[];
  placement: number | null; score: number | null; rank: Snapshot["rank"]; rankLoaded: boolean;
};
export type MatchDetailTeam = { teamId: number; win: boolean | null; objectives: { label: string; kills: number }[];
  /** Banned champion ids in pick order; missing on matches saved before bans were kept. */
  bans?: number[] };
export type MatchDetailView = { game: Game; players: MatchDetailPlayer[]; teams: MatchDetailTeam[] };
