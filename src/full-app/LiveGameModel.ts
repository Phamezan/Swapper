/** Live Game data as `live_game::model::View` serializes it, plus the pure
 *  helpers and routing rules the page, the flyout card and the shell share.
 *  No React or Tauri imports, so the flyout can use it without the full app. */

export type Rank = { tier: string; division: string; lp: number; wins: number; losses: number };
export type Form = { games: number; wins: number; kills: number; deaths: number; assists: number };
export type Recent = {
  championId: number; queue: string; win: boolean; kills: number; deaths: number; assists: number;
  cs: number; duration: number; createdAt: number; role: string;
};
export type History = { form: Form; onChampion: Form; mainRole: [string, number] | null; streak: number; recent: Recent[] };
export type Badge = { kind: "streak-win" | "streak-loss" | "new-champ" | "off-role" | "one-trick"; label: string; detail: string };
export type Live = { level: number; kills: number; deaths: number; assists: number; cs: number; wardScore: number; items: number[]; dead: boolean };
export type Player = {
  key: string; side: "blue" | "red"; championId: number; championName: string; riotId: string;
  isSelf: boolean; isBot: boolean; position: string; spells: [number, number];
  keystone: number; primaryStyle: number; secondaryStyle: number; summonerLevel: number; profileIcon: number;
  solo: Rank | null; flex: Rank | null; history: History | null; badges: Badge[];
  damage: string; status: "pending" | "ready" | "partial" | "skipped"; live: Live | null;
};
export type TeamSummary = { averageTier: string | null; averageDivision: string; ranked: number; physical: number | null };
export type LiveGameView = {
  gameId: number; queueId: number; queue: string; mapId: number; players: Player[];
  blue: TeamSummary; red: TeamSummary; gameTime: number | null;
};

export const pct = (wins: number, games: number) => (games ? Math.round(100 * wins / games) : 0);
/** The client sometimes reports 0 losses for other players; many wins and no loss is that gap, not a record
 *  (same rule as `ZERO_LOSS_WIN_LIMIT` in Rust). */
export const hasRecord = (rank: Rank) => rank.losses > 0 || rank.wins < 10;
export const kda = (f: Pick<Form, "kills" | "deaths" | "assists">) => (f.kills + f.assists) / Math.max(1, f.deaths);
export const titleCase = (text: string) => text.charAt(0) + text.slice(1).toLowerCase();
export const clock = (seconds: number) => `${Math.floor(seconds / 60)}:${String(Math.floor(seconds % 60)).padStart(2, "0")}`;

export function rankLabel(rank: Rank | null): string {
  return rank ? `${titleCase(rank.tier)} ${rank.division}`.trim() : "Unranked";
}

/** Tier -> the individual crest supported by `RankCrest` and the icon backend. */
export function crestSlug(tier: string | null | undefined): string {
  const t = (tier ?? "").toLowerCase();
  return ["iron", "bronze", "silver", "gold", "platinum", "emerald", "diamond", "master", "grandmaster", "challenger"].includes(t) ? t : "none";
}

export function splitRiotId(riotId: string): [string, string] {
  const at = riotId.lastIndexOf("#");
  return at > 0 ? [riotId.slice(0, at), riotId.slice(at)] : [riotId, ""];
}

/** Swapper role slug for an LCU / Live Client position. */
export function roleOf(position: string): string {
  return ({ TOP: "top", JUNGLE: "jungle", MIDDLE: "mid", BOTTOM: "adc", UTILITY: "support" } as Record<string, string>)[position.toUpperCase()] ?? "";
}

// ---------------------------------------------------------------------------
// Shell routing. Pages are plain strings so this stays testable from Node.
// ---------------------------------------------------------------------------

export const LIVE_PAGE = "Live Game";
const CHAMP_SELECT_PAGE = "Champion select";
const HOME = "Dashboard";

/** Where the shell goes when a game starts, and what to restore afterwards.
 *  `champSelectReturn` is the page champion select interrupted (if any): a
 *  game straight after champion select returns there, never to champion select. */
export function enterLive(current: string, liveReturn: string | null, champSelectReturn: string | null): { page: string; returnTo: string } {
  if (current === LIVE_PAGE) return { page: LIVE_PAGE, returnTo: liveReturn ?? HOME };
  const back = current === CHAMP_SELECT_PAGE ? champSelectReturn ?? HOME : current;
  return { page: LIVE_PAGE, returnTo: back === LIVE_PAGE ? HOME : back };
}

/** The page to show when the game ends: back to where the user was, but only
 *  if they are still on Live Game; otherwise they navigated on purpose. */
export function leaveLive(current: string, returnTo: string | null): string | null {
  return current === LIVE_PAGE ? returnTo ?? HOME : null;
}

/** The Live Game sidebar and search entry exists only while a game runs. */
export function showInNav(page: string, inGame: boolean): boolean {
  return inGame || page !== LIVE_PAGE;
}
