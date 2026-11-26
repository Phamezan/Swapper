import type { Account } from "../../../src/app/types";
import type { Bench, Game, LpPoint, Mmr, Player, Snapshot, Summary } from "../../../src/full-app/types";
import type { MatchView, TimelineEvent } from "../../../src/full-app/MatchDetailModel";
import type { MatchupGuide, MatchupVod } from "../../../src/full-app/matchupGuide";
import type { RunesView } from "../../../src/runes/types";
import type { Draft } from "../../../src/full-app/champ-select-model";
import {
  DD_ABILITIES_16_23,
  DD_CHAMPIONS,
  DD_ITEM_NAMES,
  DD_RUNE_NAMES,
  LOLA_BUILDS_16_23,
  LOLA_FEATURED_MATCHUPS_16_23,
  LOLA_TIER_BENCHMARKS_16_23,
  REAL_DATA_PATCH,
  REAL_DATA_VERSION,
  REAL_NOTES_PATCH,
  RIOT_PATCH_NOTES_26_22,
  RIOT_PATCH_NOTES_26_23,
  UGG_COUNTERS_16_23,
  UGG_DUOS_16_23,
  UGG_MATCHUPS_16_23,
  UGG_TIERLIST_16_23,
  type RealChampBuild,
  type RealRole,
  type RealRuneSetup,
} from "./realPatch16_23Data.ts";

export const DEMO_DDRAGON = "https://ddragon.leagueoflegends.com/cdn";
export const DEMO_VERSION = REAL_DATA_VERSION;

export type Role = RealRole;
export type RankData = { tier: string; division: string; lp: number; wins: number; losses: number };
export type RuneSetup = RealRuneSetup;

export type ChampSpec = RealChampBuild & {
  earlyComp: number;
  trinket: number;
  core: number[];
};

export const ITEM_NAMES: Record<number, string> = DD_ITEM_NAMES;

const SUPPORT_QUEST_IDS = new Set([3865, 3869, 3870, 3871, 3876, 3877]);

function normalizeSpells(spells: [number, number], role: Role): [number, number] {
  if (role === "jungle") {
    if (spells.includes(11)) return spells;
    return [4, 11];
  }
  if (spells.includes(11)) {
    return role === "top" ? [4, 12] : role === "adc" ? [4, 21] : [4, 14];
  }
  return spells;
}

function buildChampSpec(b: RealChampBuild): ChampSpec {
  const spells = normalizeSpells(b.spells, b.role);
  const altRunes = b.altRunes ? { ...b.altRunes, spells: normalizeSpells(b.altRunes.spells, b.role) } : null;
  let nonBoots = [...b.nonBoots];
  if (b.role === "support" && !nonBoots.some(id => SUPPORT_QUEST_IDS.has(id))) {
    const quest = b.damageClass === "kMagic" ? 3870 : 3869;
    nonBoots = [quest, ...nonBoots];
  }
  const earlyComp = b.role === "support" ? 3067 : b.damageClass === "kMagic" ? 1026 : b.role === "adc" ? 1037 : 1036;
  const trinket = b.role === "adc" ? 3363 : b.role === "jungle" || b.role === "support" ? 3364 : 3340;
  return {
    ...b,
    spells,
    altRunes,
    nonBoots,
    core: nonBoots,
    earlyComp,
    trinket,
  };
}

export const CHAMP_SPECS: Record<number, ChampSpec> = Object.fromEntries(
  Object.values(LOLA_BUILDS_16_23).map(b => [b.id, buildChampSpec(b)]),
);

export const DEMO_CHAMPION_NAMES: Record<string, string> = Object.fromEntries(
  Object.entries(DD_CHAMPIONS).map(([id, [name]]) => [id, name]),
);

export const VALID_PROFILE_ICONS = [
  5367, 4658, 4568, 5432, 6271, 4652, 588, 785, 1114, 1439, 1665, 2076, 3180, 3556,
  3860, 4025, 4219, 4379, 4496, 4791, 4901, 5024, 5142, 5274, 5611, 5912, 6034, 6340, 6420, 6501,
];

const EUW_RIOT_IDS = [
  "brambleleaf#EUW", "northward#EUW", "crispwave#EUW2", "glasskite#404", "hallowpine#EUW", "velvetrift#882",
  "rivermist#EUW", "cindervale#EUW", "ashbarrow#771", "coldlantern#EUW", "mintpulse#EUW2", "ironmeadow#909",
  "duskharbor#EUW", "flintcrest#108", "wrenhollow#EUW", "mossbridge#314", "thornwake#EUW", "peatstone#EUW2",
  "cedarwind#612", "silverquill#EUW", "amberford#EUW", "slatebrook#205", "frostfern#EUW", "larkspur#EUW2",
  "heatherfen#419", "kestrelrun#EUW", "oakenvale#EUW", "copperfinch#731", "starlingbay#EUW", "briarwatch#550",
  "pebbleford#EUW", "rowancliff#EUW2", "cloudstep#840", "willowbend#EUW", "fenwick#EUW", "galehaven#219",
  "alderbrook#EUW", "sorrelgate#677", "marrowhill#EUW", "reedhollow#EUW2", "cloverfield#112", "birchshore#EUW",
  "sunlitmoor#EUW", "cairnridge#903", "thistlebay#EUW", "driftwood#EUW2", "hazelmere#488", "stormcreek#EUW",
  "waverly#EUW", "nightjar#326", "pinecrest#EUW", "verdigris#705", "saltmarsh#EUW", "blackthorn#EUW2",
];

const NA_RIOT_IDS = [
  "harborbell#NA1", "silverdrake#512", "tidesmith#NA2", "cobaltpine#NA1", "redwoodfox#777", "canyonlark#NA1",
  "marigoldbay#NA1", "granitepeak#415", "prairiewind#NA2", "cedarline#NA1", "sequoia#204", "driftstone#NA1",
  "monarchvale#818", "bluejayrun#NA1", "meadowlark#NA2", "highridge#303", "sprucecove#NA1", "suncrest#NA1",
  "mesaflare#629", "clearwater#NA1", "oakhollow#NA2", "pacificfog#901", "ironwood#NA1", "bayoumist#412",
  "timberline#NA1", "sandbar#NA2", "goldfinch#520", "sierraedge#NA1", "riverstone#NA1", "aspenfall#716",
  "harborlight#NA1", "wildsage#NA2", "coyotepass#389", "northstar#NA1", "cypressbay#NA1", "summitrun#844",
  "rainshadow#NA1", "lakeshore#NA2", "ambertrail#119", "cloudpeak#NA1", "falconridge#NA1", "maplecrest#652",
  "glacierbay#NA1", "boulderpass#NA2", "copperhill#278", "windriver#NA1", "sagebrush#NA1", "starfall#930",
  "valleyforge#NA1", "birchcreek#NA2", "tidewater#504", "ironridge#NA1", "mossrock#NA1", "foxglove#608",
];

// Invented player handles and team tags (never real players or pros).
const INVENTED_PRO_HANDLES: { name: string; team: string; league: string; region: string }[] = [
  { name: "Miro", team: "VLT", league: "LEC", region: "EUW" },
  { name: "Taeyun", team: "KRN", league: "LCK", region: "KR" },
  { name: "Kaelen", team: "ARC", league: "LEC", region: "EUW" },
  { name: "Nori", team: "SOL", league: "LCK", region: "KR" },
  { name: "Dorian", team: "NVA", league: "LCS", region: "NA" },
  { name: "Soren", team: "VLT", league: "LEC", region: "EUW" },
  { name: "Haneul", team: "KRN", league: "LCK", region: "KR" },
  { name: "Vexen", team: "ARC", league: "LCS", region: "NA" },
];

export type RealisticAccountConfig = {
  account: Account;
  level: number;
  role: Role;
  rank: RankData;
  mmr: Mmr;
  todayCount?: number;
  duoPartner: { riotId: string; role: Role; iconId: number };
  squadRoster: { puuid: string; riotId: string; role: Role; iconId: number; champs: number[] }[];
  matchScripts: {
    queue: "solo" | "flex";
    win: boolean;
    duration: number;
    lpChange: number | null;
    myChamp: number;
    oppChamp: number;
    myKda: [number, number, number];
    withDuo?: boolean;
  }[];
  priorLpAnchors: { daysAgo: number; lpDeltaFromWindowStart: number }[];
  liveMatch: {
    gameTime: number;
    myChamp: number;
    allyChamps: [number, number, number, number, number];
    enemyChamps: [number, number, number, number, number];
  };
};

const iconUrl = (id: number) => `${DEMO_DDRAGON}/${DEMO_VERSION}/img/profileicon/${id}.png`;

export const REALISTIC_ACCOUNTS: RealisticAccountConfig[] = [
  {
    account: {
      id: "kestrel", name: "Kestrel", riotId: "Kestrel#EUW", nickname: null,
      platform: "EUW1", region: "EUW", profileIconId: 5367, iconDataUrl: iconUrl(5367), launchTarget: "league",
    },
    level: 287,
    role: "top",
    rank: { tier: "EMERALD", division: "II", lp: 64, wins: 88, losses: 71 },
    mmr: { tier: "EMERALD", division: "I", lp: 48, games: 10, trend: 0 },
    todayCount: 10,
    duoPartner: { riotId: "brambleleaf#EUW", role: "jungle", iconId: 5432 },
    squadRoster: [
      { puuid: "sq-k-1", riotId: "brambleleaf#EUW", role: "jungle", iconId: 5432, champs: [64, 254, 59] },
      { puuid: "sq-k-2", riotId: "rivermist#EUW", role: "mid", iconId: 6271, champs: [61, 103, 112] },
      { puuid: "sq-k-3", riotId: "duskharbor#EUW", role: "adc", iconId: 4652, champs: [222, 145, 51] },
      { puuid: "sq-k-4", riotId: "wrenhollow#EUW", role: "support", iconId: 4219, champs: [412, 111, 902] },
    ],
    matchScripts: [
      // Today (10-game win streak: 10W 0L, +216 LP total, Emerald IV 48 LP -> Emerald III -> Emerald II 64 LP)
      { queue: "solo", win: true, duration: 1712, lpChange: 23, myChamp: 266, oppChamp: 122, myKda: [9, 2, 7], withDuo: true },
      { queue: "solo", win: true, duration: 1584, lpChange: 21, myChamp: 164, oppChamp: 516, myKda: [7, 2, 9], withDuo: true },
      { queue: "solo", win: true, duration: 1842, lpChange: 24, myChamp: 266, oppChamp: 114, myKda: [10, 3, 6], withDuo: true },
      { queue: "solo", win: true, duration: 1626, lpChange: 22, myChamp: 875, oppChamp: 58,  myKda: [8, 3, 8] },
      { queue: "solo", win: true, duration: 1518, lpChange: 19, myChamp: 24,  oppChamp: 897, myKda: [6, 2, 7] },
      { queue: "solo", win: true, duration: 1794, lpChange: 23, myChamp: 266, oppChamp: 82,  myKda: [11, 2, 5], withDuo: true },
      { queue: "solo", win: true, duration: 1668, lpChange: 21, myChamp: 164, oppChamp: 126, myKda: [7, 3, 10] },
      { queue: "solo", win: true, duration: 1896, lpChange: 22, myChamp: 58,  oppChamp: 150, myKda: [9, 4, 7] },
      { queue: "solo", win: true, duration: 1542, lpChange: 20, myChamp: 266, oppChamp: 98,  myKda: [6, 1, 9], withDuo: true },
      { queue: "solo", win: true, duration: 1738, lpChange: 21, myChamp: 875, oppChamp: 54,  myKda: [8, 3, 6] },
      // Yesterday evening (4 games: 3W 1L, 1 Flex)
      { queue: "solo", win: true,  duration: 2110, lpChange: 22,   myChamp: 875, oppChamp: 58,  myKda: [9, 4, 8] },
      { queue: "flex", win: true,  duration: 1640, lpChange: null, myChamp: 266, oppChamp: 150, myKda: [7, 2, 6], withDuo: true },
      { queue: "solo", win: false, duration: 1482, lpChange: -19,  myChamp: 164, oppChamp: 24,  myKda: [3, 5, 4] },
      { queue: "solo", win: true,  duration: 1866, lpChange: 24,   myChamp: 266, oppChamp: 897, myKda: [10, 3, 6] },
      // 2 days ago (1 game: 0W 1L)
      { queue: "solo", win: false, duration: 2034, lpChange: -17,  myChamp: 58,  oppChamp: 82,  myKda: [5, 7, 4] },
    ],
    priorLpAnchors: [
      { daysAgo: 7, lpDeltaFromWindowStart: -22 },
      { daysAgo: 12, lpDeltaFromWindowStart: -48 },
      { daysAgo: 18, lpDeltaFromWindowStart: -68 },
      { daysAgo: 25, lpDeltaFromWindowStart: -96 },
    ],
    liveMatch: {
      gameTime: 862,
      myChamp: 266,
      allyChamps: [266, 64, 61, 222, 412],
      enemyChamps: [122, 254, 238, 81, 111],
    },
  },
  {
    account: {
      id: "vesper", name: "Vesper", riotId: "Vesper#EUW", nickname: null,
      platform: "EUW1", region: "EUW", profileIconId: 4658, iconDataUrl: iconUrl(4658), launchTarget: "league",
    },
    level: 164,
    role: "mid",
    rank: { tier: "PLATINUM", division: "I", lp: 28, wins: 54, losses: 46 },
    mmr: { tier: "PLATINUM", division: "I", lp: 64, games: 10, trend: 18 },
    duoPartner: { riotId: "cindervale#EUW", role: "support", iconId: 4791 },
    squadRoster: [
      { puuid: "sq-v-1", riotId: "flintcrest#108", role: "top", iconId: 4901, champs: [516, 266, 98] },
      { puuid: "sq-v-2", riotId: "ashbarrow#771", role: "jungle", iconId: 5024, champs: [59, 113, 64] },
      { puuid: "sq-v-3", riotId: "amberford#EUW", role: "adc", iconId: 5142, champs: [498, 202, 22] },
      { puuid: "sq-v-4", riotId: "cindervale#EUW", role: "support", iconId: 4791, champs: [497, 117, 43] },
    ],
    matchScripts: [
      // Today (3 games: 2W 1L, +22 -18 +21 = +25 LP)
      { queue: "solo", win: true,  duration: 1648, lpChange: 22,   myChamp: 103, oppChamp: 238, myKda: [9, 2, 8], withDuo: true },
      { queue: "solo", win: false, duration: 1890, lpChange: -18,  myChamp: 61,  oppChamp: 517, myKda: [4, 5, 7] },
      { queue: "solo", win: true,  duration: 1756, lpChange: 21,   myChamp: 103, oppChamp: 112, myKda: [7, 3, 10], withDuo: true },
      // Yesterday evening (4 games)
      { queue: "solo", win: true,  duration: 2042, lpChange: 23,   myChamp: 112, oppChamp: 61,  myKda: [8, 4, 9] },
      { queue: "flex", win: true,  duration: 1528, lpChange: null, myChamp: 103, oppChamp: 157, myKda: [10, 2, 7], withDuo: true },
      { queue: "solo", win: false, duration: 1612, lpChange: -19,  myChamp: 517, oppChamp: 7,   myKda: [5, 7, 4] },
      { queue: "solo", win: true,  duration: 1834, lpChange: 20,   myChamp: 61,  oppChamp: 777, myKda: [6, 2, 11] },
      // 2 days ago (3 games)
      { queue: "solo", win: false, duration: 1978, lpChange: -17,  myChamp: 103, oppChamp: 163, myKda: [4, 5, 6] },
      { queue: "solo", win: true,  duration: 1564, lpChange: 22,   myChamp: 238, oppChamp: 268, myKda: [11, 3, 5] },
      { queue: "flex", win: false, duration: 1742, lpChange: null, myChamp: 61,  oppChamp: 84,  myKda: [3, 6, 8] },
      // 4 days ago (2 games)
      { queue: "solo", win: true,  duration: 2180, lpChange: 21,   myChamp: 103, oppChamp: 4,   myKda: [8, 3, 9] },
      { queue: "solo", win: false, duration: 1420, lpChange: -19,  myChamp: 112, oppChamp: 3,   myKda: [3, 5, 5] },
    ],
    priorLpAnchors: [
      { daysAgo: 7, lpDeltaFromWindowStart: -14 },
      { daysAgo: 12, lpDeltaFromWindowStart: -38 },
      { daysAgo: 18, lpDeltaFromWindowStart: -52 },
      { daysAgo: 25, lpDeltaFromWindowStart: -64 },
    ],
    liveMatch: {
      gameTime: 834,
      myChamp: 103,
      allyChamps: [516, 59, 103, 498, 497],
      enemyChamps: [164, 56, 238, 202, 412],
    },
  },
  {
    account: {
      id: "sable", name: "Sable", riotId: "Sable#NA1", nickname: null,
      platform: "NA1", region: "NA", profileIconId: 4568, iconDataUrl: iconUrl(4568), launchTarget: "riot_client",
    },
    level: 218,
    role: "adc",
    rank: { tier: "DIAMOND", division: "IV", lp: 42, wins: 63, losses: 51 },
    mmr: { tier: "DIAMOND", division: "IV", lp: 78, games: 10, trend: 22 },
    duoPartner: { riotId: "harborbell#NA1", role: "support", iconId: 5274 },
    squadRoster: [
      { puuid: "sq-s-1", riotId: "granitepeak#415", role: "top", iconId: 5611, champs: [164, 897, 150] },
      { puuid: "sq-s-2", riotId: "silverdrake#512", role: "jungle", iconId: 5912, champs: [254, 104, 5] },
      { puuid: "sq-s-3", riotId: "cobaltpine#NA1", role: "mid", iconId: 6034, champs: [112, 163, 61] },
      { puuid: "sq-s-4", riotId: "harborbell#NA1", role: "support", iconId: 5274, champs: [902, 117, 201] },
    ],
    matchScripts: [
      // Today (3 games: 2W 1L, +24 -19 +22 = +27 LP)
      { queue: "solo", win: true,  duration: 1824, lpChange: 24,   myChamp: 222, oppChamp: 145, myKda: [11, 2, 9], withDuo: true },
      { queue: "solo", win: false, duration: 1510, lpChange: -19,  myChamp: 145, oppChamp: 51,  myKda: [5, 6, 4] },
      { queue: "solo", win: true,  duration: 1962, lpChange: 22,   myChamp: 222, oppChamp: 202, myKda: [9, 3, 10], withDuo: true },
      // Yesterday evening (4 games)
      { queue: "solo", win: true,  duration: 1688, lpChange: 23,   myChamp: 51,  oppChamp: 81,  myKda: [8, 2, 7] },
      { queue: "flex", win: true,  duration: 1790, lpChange: null, myChamp: 222, oppChamp: 22,  myKda: [10, 3, 8], withDuo: true },
      { queue: "solo", win: true,  duration: 2145, lpChange: 21,   myChamp: 145, oppChamp: 498, myKda: [12, 4, 7] },
      { queue: "solo", win: false, duration: 1465, lpChange: -20,  myChamp: 81,  oppChamp: 110, myKda: [4, 5, 5] },
      // 2 days ago (3 games)
      { queue: "solo", win: true,  duration: 1872, lpChange: 22,   myChamp: 222, oppChamp: 18,  myKda: [8, 3, 11] },
      { queue: "solo", win: false, duration: 2088, lpChange: -18,  myChamp: 51,  oppChamp: 523, myKda: [6, 6, 5] },
      { queue: "flex", win: true,  duration: 1618, lpChange: null, myChamp: 498, oppChamp: 67,  myKda: [7, 2, 9] },
      // 4 days ago (2 games)
      { queue: "solo", win: false, duration: 1592, lpChange: -19,  myChamp: 145, oppChamp: 96,  myKda: [5, 7, 4] },
      { queue: "solo", win: true,  duration: 1934, lpChange: 23,   myChamp: 222, oppChamp: 145, myKda: [10, 3, 8] },
    ],
    priorLpAnchors: [
      { daysAgo: 7, lpDeltaFromWindowStart: -22 },
      { daysAgo: 12, lpDeltaFromWindowStart: -48 },
      { daysAgo: 18, lpDeltaFromWindowStart: -66 },
      { daysAgo: 25, lpDeltaFromWindowStart: -95 },
    ],
    liveMatch: {
      gameTime: 896,
      myChamp: 222,
      allyChamps: [164, 254, 112, 222, 902],
      enemyChamps: [897, 104, 163, 145, 111],
    },
  },
];

// --- Ladder helpers ---
const LADDER = ["IRON", "BRONZE", "SILVER", "GOLD", "PLATINUM", "EMERALD", "DIAMOND"];
const DIVS = ["IV", "III", "II", "I"];
function toScore(r: { tier: string; division: string; lp: number }) {
  const t = Math.max(0, LADDER.indexOf(r.tier));
  const d = Math.max(0, DIVS.indexOf(r.division));
  return t * 400 + d * 100 + r.lp;
}
function fromScore(score: number): { tier: string; division: string; lp: number } {
  const clamped = Math.max(0, Math.min(LADDER.length * 400 - 1, Math.round(score)));
  const t = Math.floor(clamped / 400);
  const rem = clamped % 400;
  return { tier: LADDER[t], division: DIVS[Math.floor(rem / 100)], lp: rem % 100 };
}

function hash(n: number) {
  let x = (n | 0) ^ 0x9e3779b9;
  x = Math.imul(x ^ (x >>> 16), 0x85ebca6b);
  x = Math.imul(x ^ (x >>> 13), 0xc2b2ae35);
  return ((x ^ (x >>> 16)) >>> 0) / 4294967296;
}
function rangeInt(seed: number, min: number, max: number) {
  return min + Math.floor(hash(seed) * (max - min + 1));
}

const ROLE_POOL: Record<Role, number[]> = {
  top: [266, 164, 875, 58, 24, 122, 516, 114, 150, 897, 82, 98, 54, 126, 86],
  jungle: [64, 254, 59, 5, 104, 56, 131, 113, 60, 120, 76, 11],
  mid: [103, 61, 112, 238, 517, 157, 777, 7, 163, 84, 268, 4, 3],
  adc: [222, 145, 51, 81, 498, 202, 22, 523, 110, 18, 96, 67],
  support: [412, 111, 89, 902, 117, 43, 497, 201, 526, 432, 888, 53, 25],
};
const ROLES: Role[] = ["top", "jungle", "mid", "adc", "support"];

/** Build items from real 16.23 Lolalytics builds scaled to game duration and gold. */
function itemsForDuration(spec: ChampSpec, durationSec: number, gold: number, isLive = false, roleOverride?: Role): number[] {
  const role = roleOverride ?? spec.role;
  const mins = durationSec / 60;
  const starter0 = spec.starters[0]?.[0] ?? (role === "support" ? 3865 : 1055);
  const nonBoots = role === "support"
    ? (spec.nonBoots.some(id => SUPPORT_QUEST_IDS.has(id)) ? spec.nonBoots : [spec.damageClass === "kMagic" ? 3870 : 3869, ...spec.nonBoots])
    : spec.nonBoots.filter(id => !SUPPORT_QUEST_IDS.has(id));
  const safeStarter = role === "support" ? 3865 : SUPPORT_QUEST_IDS.has(starter0) ? 1056 : starter0;
  const trinket = role === "adc" ? 3363 : role === "jungle" || role === "support" ? 3364 : 3340;
  if (isLive) {
    if (role === "support") return [nonBoots[0] ?? 3869, spec.boots, spec.earlyComp, 2055, 0, 0, trinket];
    const liveStarter = role === "jungle" ? 2031 : safeStarter;
    return [nonBoots[0] ?? spec.items[0] ?? 0, spec.boots, liveStarter, spec.earlyComp, 0, 0, trinket];
  }
  const slots: number[] = [];
  if (role === "support") {
    const legCount = mins >= 33 ? 3 : mins >= 25 ? 2 : 1;
    slots.push(nonBoots[0] ?? 3869, spec.boots);
    for (let i = 1; i <= legCount && i < nonBoots.length; i++) slots.push(nonBoots[i]);
    if (slots.length < 6) slots.push(spec.earlyComp);
    if (slots.length < 6) slots.push(2055);
  } else {
    const completed = gold >= 15800 ? 5 : gold >= 13200 ? 4 : gold >= 10400 ? 3 : 2;
    for (let i = 0; i < Math.min(completed, nonBoots.length); i++) slots.push(nonBoots[i]);
    slots.push(spec.boots);
    if (slots.length < 6 && mins < 28) slots.push(spec.earlyComp);
    if (slots.length < 6 && mins < 24 && role !== "jungle" && !slots.includes(safeStarter)) slots.push(safeStarter);
  }
  while (slots.length < 6) slots.push(0);
  return [...slots.slice(0, 6), trinket];
}

function buildSessionTimestamps(now: number, durations: number[], accountOffset: number, todayCount = 3): number[] {
  const d = new Date(now);
  const midnight = new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const elapsedToday = now - midnight;
  const end0 = now - (14 + accountOffset * 5) * 60_000;
  const gap = todayCount > 5 ? 8 * 60_000 : elapsedToday < 4 * 36e5 ? 10 * 60_000 : 14 * 60_000;

  // Build newest-first start times for today's session; if wall clock is very early morning,
  // compress today's timestamps proportionally inside [midnight + 60_000, end0] so all games stay today.
  const rawStarts: number[] = [];
  let cursor = end0;
  for (let i = 0; i < todayCount && i < durations.length; i++) {
    const start = cursor - (durations[i] ?? 1700) * 1000;
    rawStarts.push(start);
    cursor = start - gap;
  }
  const minAllowed = midnight + 60_000;
  const earliestRaw = rawStarts[rawStarts.length - 1] ?? minAllowed;
  let todayStarts = rawStarts;
  if (earliestRaw < minAllowed && rawStarts.length > 1) {
    const rawSpan = Math.max(1, end0 - earliestRaw);
    const availSpan = Math.max(60_000, end0 - minAllowed);
    const scale = availSpan / rawSpan;
    todayStarts = rawStarts.map((s) => Math.round(end0 - (end0 - s) * scale));
  } else if (earliestRaw < minAllowed) {
    todayStarts = [minAllowed];
  }

  const evening = (daysBack: number, hour: number, minute: number) =>
    new Date(d.getFullYear(), d.getMonth(), d.getDate() - daysBack, hour, minute).getTime();

  const priorStarts = [
    evening(1, 22, 18 - accountOffset * 5),
    evening(1, 21, 34 - accountOffset * 4),
    evening(1, 20, 49 - accountOffset * 3),
    evening(1, 20, 4 - accountOffset * 2),
    evening(2, 21, 52 - accountOffset * 6),
    evening(2, 21, 8 - accountOffset * 4),
    evening(2, 20, 19 - accountOffset * 3),
    evening(4, 22, 11 - accountOffset * 5),
    evening(4, 21, 23 - accountOffset * 4),
  ];

  return [...todayStarts, ...priorStarts].slice(0, durations.length);
}

export type ExtendedPlayer = Player & {
  profileIcon: number;
  level: number;
  spells: [number, number];
  runes: RuneSetup;
  rank: RankData;
  summonerLevel: number;
  placement: number;
  score: number;
};

export type BuiltMatch = Game & {
  players: ExtendedPlayer[];
  bans: [number[], number[]];
};

export type BuiltAccountBundle = {
  config: RealisticAccountConfig;
  snapshot: Snapshot;
  matches: BuiltMatch[];
  liveView: Record<string, unknown>;
  draftView: { draft: Draft; runes: RunesView };
  playerDirectory: Map<string, { riotId: string; iconId: number; level: number; rank: RankData; region: string }>;
};

function makeLobbyRank(baseScore: number, seed: number): RankData {
  const offset = rangeInt(seed, -135, 145);
  const pos = fromScore(baseScore + offset);
  const games = rangeInt(seed + 11, 55, 240);
  const wr = 0.47 + hash(seed + 19) * 0.11;
  const wins = Math.round(games * wr);
  return { ...pos, wins, losses: games - wins };
}

function runeName(id: number): string {
  return DD_RUNE_NAMES[id] ?? "Keystone";
}

function buildAccountBundle(cfg: RealisticAccountConfig, accIndex: number, now: number): BuiltAccountBundle {
  const baseScore = toScore(cfg.rank);
  const idPool = cfg.account.region === "NA" ? NA_RIOT_IDS : EUW_RIOT_IDS;
  const timestamps = buildSessionTimestamps(now, cfg.matchScripts.map(s => s.duration), accIndex, cfg.todayCount ?? 3);
  const playerDirectory = new Map<string, { riotId: string; iconId: number; level: number; rank: RankData; region: string }>();

  playerDirectory.set(cfg.account.riotId!.toLowerCase(), {
    riotId: cfg.account.riotId!, iconId: cfg.account.profileIconId ?? 5367, level: cfg.level, rank: cfg.rank, region: cfg.account.region ?? "EUW",
  });

  const matches: BuiltMatch[] = cfg.matchScripts.map((script, gi) => {
    const seedBase = (accIndex + 1) * 10000 + gi * 317;
    const mins = script.duration / 60;
    const usedChamps = new Set<number>([script.myChamp, script.oppChamp]);
    const myRoleIdx = ROLES.indexOf(cfg.role);

    const pickUnique = (role: Role, s: number) => {
      const pool = ROLE_POOL[role];
      for (let k = 0; k < pool.length; k++) {
        const cand = pool[(Math.floor(hash(s + k * 7) * pool.length) + k) % pool.length];
        if (!usedChamps.has(cand)) { usedChamps.add(cand); return cand; }
      }
      return pool[0];
    };

    const allyChamps: number[] = ROLES.map((r, ri) => ri === myRoleIdx ? script.myChamp : pickUnique(r, seedBase + ri * 13));
    const enemyChamps: number[] = ROLES.map((r, ri) => ri === myRoleIdx ? script.oppChamp : pickUnique(r, seedBase + 100 + ri * 17));
    const banPool = Object.keys(CHAMP_SPECS).map(Number).filter(id => !usedChamps.has(id));
    const bansBlue = Array.from({ length: 5 }, (_, bi) => banPool[(gi * 7 + bi * 3 + accIndex * 5) % banPool.length]);
    const bansRed = Array.from({ length: 5 }, (_, bi) => banPool[(gi * 7 + bi * 3 + 17 + accIndex * 5) % banPool.length]);

    const usedIds = new Set<string>([cfg.account.riotId!]);
    const pickRiotId = (s: number) => {
      for (let k = 0; k < idPool.length; k++) {
        const cand = idPool[(Math.floor(hash(s + k * 11) * idPool.length) + k) % idPool.length];
        if (!usedIds.has(cand)) { usedIds.add(cand); return cand; }
      }
      return idPool[0];
    };

    const allyKills = [0, 0, 0, 0, 0];
    const allyDeaths = [0, 0, 0, 0, 0];
    const enemyKills = [0, 0, 0, 0, 0];
    const enemyDeaths = [0, 0, 0, 0, 0];

    allyKills[myRoleIdx] = script.myKda[0];
    allyDeaths[myRoleIdx] = script.myKda[1];

    const winTeamTarget = rangeInt(seedBase + 5, 22, 34);
    const loseTeamTarget = Math.max(12, winTeamTarget - rangeInt(seedBase + 9, 5, 14));
    const allyTargetKills = script.win ? winTeamTarget : loseTeamTarget;
    const enemyTargetKills = script.win ? loseTeamTarget : winTeamTarget;

    const roleKillWeight = [0.22, 0.22, 0.24, 0.25, 0.07];
    const distribute = (arr: number[], fixedIdx: number, total: number, s: number) => {
      let rem = Math.max(0, total - (fixedIdx >= 0 ? arr[fixedIdx] : 0));
      const free = [0, 1, 2, 3, 4].filter(i => i !== fixedIdx);
      const weights = free.map(i => roleKillWeight[i] * (0.75 + hash(s + i * 29) * 0.55));
      const wSum = weights.reduce((a, b) => a + b, 0);
      free.forEach((idx, k) => {
        const v = k === free.length - 1 ? rem : Math.min(rem, Math.round((total - (fixedIdx >= 0 ? arr[fixedIdx] : 0)) * weights[k] / wSum));
        arr[idx] = Math.max(idx === 4 ? 0 : 1, v);
        rem = Math.max(0, rem - arr[idx]);
      });
    };

    distribute(allyKills, myRoleIdx, allyTargetKills, seedBase + 41);
    distribute(enemyKills, -1, enemyTargetKills, seedBase + 61);
    const totalAllyKills = allyKills.reduce((a, b) => a + b, 0);
    const totalEnemyKills = enemyKills.reduce((a, b) => a + b, 0);

    const distributeDeaths = (arr: number[], fixedIdx: number, total: number, s: number) => {
      let rem = Math.max(0, total - (fixedIdx >= 0 ? arr[fixedIdx] : 0));
      const free = [0, 1, 2, 3, 4].filter(i => i !== fixedIdx);
      free.forEach((idx, k) => {
        if (k === free.length - 1) { arr[idx] = Math.max(1, rem); return; }
        const avg = rem / (free.length - k);
        const v = Math.max(1, Math.min(rem - (free.length - k - 1), Math.round(avg + (hash(s + idx * 31) - 0.5) * 3)));
        arr[idx] = v;
        rem -= v;
      });
    };
    distributeDeaths(allyDeaths, myRoleIdx, totalEnemyKills, seedBase + 81);
    distributeDeaths(enemyDeaths, -1, totalAllyKills, seedBase + 97);
    const fixDeathIdx = myRoleIdx === 4 ? 3 : 4;
    allyDeaths[fixDeathIdx] = Math.max(1, allyDeaths[fixDeathIdx] + (totalEnemyKills - allyDeaths.reduce((a, b) => a + b, 0)));
    enemyDeaths[4] = Math.max(1, enemyDeaths[4] + (totalAllyKills - enemyDeaths.reduce((a, b) => a + b, 0)));

    const makeParticipant = (p: number): ExtendedPlayer => {
      const ally = p < 5;
      const ri = p % 5;
      const role = ROLES[ri];
      const isMe = ally && ri === myRoleIdx;
      const champId = ally ? allyChamps[ri] : enemyChamps[ri];
      const spec = CHAMP_SPECS[champId] ?? CHAMP_SPECS[266];
      const won = ally ? script.win : !script.win;
      const kills = ally ? allyKills[ri] : enemyKills[ri];
      const deaths = ally ? allyDeaths[ri] : enemyDeaths[ri];
      const teamKills = ally ? totalAllyKills : totalEnemyKills;
      const assists = isMe
        ? script.myKda[2]
        : Math.max(1, Math.min(Math.max(1, teamKills - kills), Math.round((teamKills - kills) * (role === "support" ? 0.68 : role === "jungle" ? 0.52 : 0.38) * (0.78 + hash(seedBase + p * 43) * 0.42))));

      let riotId: string;
      let profileIcon: number;
      if (isMe) {
        riotId = cfg.account.riotId!;
        profileIcon = cfg.account.profileIconId ?? 5367;
      } else if (ally && script.withDuo && role === cfg.duoPartner.role) {
        riotId = cfg.duoPartner.riotId;
        profileIcon = cfg.duoPartner.iconId;
        usedIds.add(riotId);
      } else {
        riotId = pickRiotId(seedBase + p * 53);
        profileIcon = VALID_PROFILE_ICONS[(seedBase + p * 17) % VALID_PROFILE_ICONS.length];
      }

      const csPerMin = role === "support"
        ? 0.9 + hash(seedBase + p * 67) * 0.45
        : role === "jungle"
          ? (won ? 5.8 : 5.0) + hash(seedBase + p * 67) * 0.85
          : (won ? 6.9 : 5.9) + hash(seedBase + p * 67) * 1.15;
      const cs = Math.round(csPerMin * mins);

      const baseLevel = mins >= 34 ? 18 : mins >= 30 ? 17 : mins >= 26 ? 16 : mins >= 23 ? 15 : 14;
      const roleLvlOffset = role === "support" ? -2 : role === "adc" || role === "jungle" ? -1 : 0;
      const level = Math.max(11, Math.min(18, baseLevel + roleLvlOffset + (won ? 0 : -1) + (kills >= 8 ? 1 : 0)));

      const gpm = role === "support"
        ? (won ? 250 : 215) + assists * 2.2
        : (won ? 368 : 312) + (csPerMin - 6.5) * 14 + kills * 7.5 + assists * 2.4;
      const gold = Math.round(gpm * mins / 10) * 10;

      const dpm = role === "support"
        ? (spec.damageClass === "kMagic" && [43, 25, 432].includes(champId) ? 390 : 240) + hash(seedBase + p * 79) * 85
        : (won ? 630 : 495) + kills * 24 + hash(seedBase + p * 79) * 150;
      const damage = Math.round(dpm * mins / 50) * 50;

      const vpm = role === "support" ? 2.0 + hash(seedBase + p * 83) * 0.5 : role === "jungle" ? 0.95 + hash(seedBase + p * 83) * 0.35 : 0.55 + hash(seedBase + p * 83) * 0.3;
      const vision = Math.round(vpm * mins);

      const items = itemsForDuration(spec, script.duration, gold, false, role);
      const rank = isMe ? cfg.rank : makeLobbyRank(baseScore, seedBase + p * 97);
      const summonerLevel = isMe ? cfg.level : rangeInt(seedBase + p * 101, 74, 540);

      if (!playerDirectory.has(riotId.toLowerCase())) {
        playerDirectory.set(riotId.toLowerCase(), { riotId, iconId: profileIcon, level: summonerLevel, rank, region: cfg.account.region ?? "EUW" });
      }

      const kdaNorm = Math.min(7, (kills + assists * 0.7) / Math.max(1, deaths));
      const farmNorm = role === "support" ? Math.min(2.6, vpm) * 6 : Math.min(9, csPerMin) * 1.9;
      const dmgNorm = role === "support" ? Math.min(10, dpm / 45) : Math.min(18, dpm / 55);
      const rawScore = 30 + (won ? 14 : 0) + kdaNorm * 4.4 + farmNorm + dmgNorm + (hash(seedBase + p * 131) - 0.5) * 6;
      return {
        championId: champId, riotId, me: isMe, ally, role,
        kills, deaths, assists, cs, damage, gold, vision, items,
        profileIcon, level, spells: normalizeSpells(spec.spells, role), runes: spec.runes, rank, summonerLevel,
        placement: 5, score: Math.round(Math.max(36, Math.min(93, rawScore))),
      };
    };

    const players = Array.from({ length: 10 }, (_, p) => makeParticipant(p));
    const rankedIdx = players.map((pl, idx) => ({ idx, s: pl.score })).sort((a, b) => b.s - a.s);
    rankedIdx.forEach((entry, rankPos) => { players[entry.idx].placement = rankPos + 1; });

    const me = players[myRoleIdx];
    const allyDmg = players.slice(0, 5).reduce((s, p) => s + p.damage, 0);
    const kp = totalAllyKills ? Math.min(100, 100 * (me.kills + me.assists) / totalAllyKills) : 0;
    const dmgShare = allyDmg ? 100 * me.damage / allyDmg : 0;
    const kdaVal = (me.kills + me.assists) / Math.max(1, me.deaths);

    return {
      id: 4200000 + accIndex * 1000 + gi,
      queue: script.queue,
      championId: me.championId,
      createdAt: timestamps[gi],
      duration: script.duration,
      win: script.win,
      kills: me.kills,
      deaths: me.deaths,
      assists: me.assists,
      metrics: [kdaVal, kp, me.cs / mins, me.damage / mins, dmgShare, me.vision / mins],
      items: me.items,
      spells: me.spells,
      keystone: me.runes.keystone,
      cs: me.cs,
      level: me.level,
      role: cfg.role,
      placement: me.placement,
      lpChange: script.lpChange,
      players,
      opponent: 5 + myRoleIdx,
      bans: [bansBlue, bansRed],
    };
  });

  // Backwards-reconstruct exact LP history ending at cfg.rank.
  const soloNewest = matches.filter(m => m.queue === "solo" && m.lpChange != null);
  const lpPointsRev: LpPoint[] = [];
  let scoreCursor = baseScore;
  let winsCursor = cfg.rank.wins;
  let lossesCursor = cfg.rank.losses;

  for (let i = 0; i < soloNewest.length; i++) {
    const m = soloNewest[i];
    const pos = fromScore(scoreCursor);
    lpPointsRev.push({
      at: m.createdAt + m.duration * 1000 + 30_000,
      tier: pos.tier, division: pos.division, lp: pos.lp,
      wins: winsCursor, losses: lossesCursor,
    });
    scoreCursor -= m.lpChange!;
    if (m.win) winsCursor = Math.max(0, winsCursor - 1);
    else lossesCursor = Math.max(0, lossesCursor - 1);
  }
  const oldestSolo = soloNewest[soloNewest.length - 1];
  const windowStartPos = fromScore(scoreCursor);
  lpPointsRev.push({
    at: oldestSolo.createdAt - 18 * 60_000,
    tier: windowStartPos.tier, division: windowStartPos.division, lp: windowStartPos.lp,
    wins: winsCursor, losses: lossesCursor,
  });
  for (const anchor of cfg.priorLpAnchors) {
    const pos = fromScore(scoreCursor + anchor.lpDeltaFromWindowStart);
    const estGames = Math.max(4, Math.round(anchor.daysAgo * 1.4));
    const estW = Math.max(0, winsCursor - Math.round(estGames * 0.54));
    const estL = Math.max(0, lossesCursor - Math.round(estGames * 0.46));
    lpPointsRev.push({
      at: now - anchor.daysAgo * 864e5,
      tier: pos.tier, division: pos.division, lp: pos.lp,
      wins: estW, losses: estL,
    });
  }
  const lpHistory = lpPointsRev.reverse();

  const summarize = (list: BuiltMatch[]): Summary => {
    const byChamp = new Map<number, { wins: number; losses: number; k: number; d: number; a: number }>();
    for (const g of list) {
      const prev = byChamp.get(g.championId) ?? { wins: 0, losses: 0, k: 0, d: 0, a: 0 };
      byChamp.set(g.championId, {
        wins: prev.wins + (g.win ? 1 : 0), losses: prev.losses + (g.win ? 0 : 1),
        k: prev.k + g.kills, d: prev.d + g.deaths, a: prev.a + g.assists,
      });
    }
    const champions = [...byChamp.entries()]
      .map(([id, s]) => ({ id, wins: s.wins, losses: s.losses, kda: Number(((s.k + s.a) / Math.max(1, s.d)).toFixed(2)) }))
      .sort((a, b) => (b.wins + b.losses) - (a.wins + a.losses));
    const roles = ROLES.map(role => {
      const sub = list.filter(g => g.role === role);
      return { role, wins: sub.filter(g => g.win).length, games: sub.length };
    });
    return { metrics: [], roles, champions };
  };

  const snapshot: Snapshot = {
    championNames: DEMO_CHAMPION_NAMES,
    summoner: { riotId: cfg.account.riotId!, level: cfg.level, icon: cfg.account.iconDataUrl },
    rank: cfg.rank,
    fetchedAt: now - 95_000,
    mmr: cfg.mmr,
    games: matches,
    lpHistory,
    summaries: {
      all: summarize(matches),
      solo: summarize(matches.filter(m => m.queue === "solo")),
      flex: summarize(matches.filter(m => m.queue === "flex")),
      normal: summarize([]),
    },
  };

  // --- Live Game (invented players, real 16.23 builds/runes/spells/damage splits/benchmarks) ---
  const lm = cfg.liveMatch;
  const liveSeed = (accIndex + 1) * 7777;
  const myLiveIdx = ROLES.indexOf(cfg.role);
  const allLiveChamps = [...lm.allyChamps, ...lm.enemyChamps];
  const liveBlueKills  = [2, 2, 1, 2, 0];
  const liveBlueDeaths = [1, 1, 2, 1, 1];
  const liveRedKills   = [1, 2, 2, 1, 0];
  const liveRedDeaths  = [2, 1, 1, 2, 1];
  const liveUsedIds = new Set<string>([cfg.account.riotId!]);
  const livePlayers = allLiveChamps.map((champId, p) => {
    const side = p < 5 ? "blue" : "red";
    const ri = p % 5;
    const role = ROLES[ri];
    const isSelf = p === myLiveIdx;
    const spec = CHAMP_SPECS[champId] ?? CHAMP_SPECS[266];
    const bench = spec.bench ?? { kills: 6, deaths: 5, assists: 7, damage: 20000, gold: 11000, cs: 185, games: 10000 };
    let riotId = cfg.account.riotId!;
    if (!isSelf) {
      if (side === "blue" && role === cfg.duoPartner.role) {
        riotId = cfg.duoPartner.riotId;
        liveUsedIds.add(riotId);
      } else {
        for (let k = 0; k < idPool.length; k++) {
          const cand = idPool[(liveSeed + p * 7 + k * 5) % idPool.length];
          if (!liveUsedIds.has(cand)) { liveUsedIds.add(cand); riotId = cand; break; }
        }
      }
    }
    const solo = isSelf ? cfg.rank : makeLobbyRank(baseScore, liveSeed + p * 41);
    const flex = p % 3 === 0 ? makeLobbyRank(baseScore - 220, liveSeed + p * 67) : null;
    const isOffRole = p === 4 && !isSelf;
    const mainRole: [Role, number] = isOffRole ? ["mid", 14] : [role, rangeInt(liveSeed + p * 19, 14, 19)];
    const formWins = isSelf ? Math.round(20 * (cfg.rank.wins / (cfg.rank.wins + cfg.rank.losses))) : rangeInt(liveSeed + p * 23, 8, 14);
    const ownChampMatches = isSelf ? matches.filter(m => m.championId === champId) : [];
    const champGames = isSelf ? ownChampMatches.length : rangeInt(liveSeed + p * 29, 2, 14);
    const champWins = isSelf
      ? ownChampMatches.filter(m => m.win).length
      : Math.min(champGames, Math.max(0, Math.round(champGames * (0.44 + hash(liveSeed + p * 31) * 0.24))));
    const ownChampKills = ownChampMatches.reduce((s, m) => s + m.kills, 0);
    const ownChampDeaths = ownChampMatches.reduce((s, m) => s + m.deaths, 0);
    const ownChampAssists = ownChampMatches.reduce((s, m) => s + m.assists, 0);
    const kills = side === "blue" ? liveBlueKills[ri] : liveRedKills[ri];
    const deaths = side === "blue" ? liveBlueDeaths[ri] : liveRedDeaths[ri];
    const assists = role === "support" ? rangeInt(liveSeed + p * 37, 4, 6) : rangeInt(liveSeed + p * 37, 1, 4);
    const liveLvl = role === "support" ? 8 : role === "adc" || role === "jungle" ? 9 : (kills >= 2 ? 11 : 10);
    const liveCs = role === "support" ? rangeInt(liveSeed + p * 43, 18, 25) : role === "jungle" ? rangeInt(liveSeed + p * 43, 82, 94) : rangeInt(liveSeed + p * 43, 102, 126);
    const badges: { kind: string; label: string; detail: string }[] = [];
    if (p === 1 && !isSelf) badges.push({ kind: "streak-win", label: "4 win streak", detail: "Won the last 4 ranked games" });
    if (p === 2 && !isSelf && champGames >= 10) badges.push({ kind: "one-trick", label: "One-trick", detail: `${Math.round(champGames / 20 * 100)}% of recent games on ${spec.name}` });
    if (isOffRole) badges.push({ kind: "off-role", label: "Off-role", detail: "Usually plays Mid" });
    if (p === 6) badges.push({ kind: "streak-loss", label: "3 loss streak", detail: "Lost the last 3 ranked games" });

    return {
      key: `live-${cfg.account.id}-${p}`,
      side, championId: champId, championName: spec.name, riotId, isSelf, isBot: false,
      position: role, spells: normalizeSpells(spec.spells, role), keystone: spec.runes.keystone,
      primaryStyle: spec.runes.primaryPageId, secondaryStyle: spec.runes.secondaryPageId,
      summonerLevel: isSelf ? cfg.level : rangeInt(liveSeed + p * 53, 82, 488),
      profileIcon: isSelf ? (cfg.account.profileIconId ?? 5367) : VALID_PROFILE_ICONS[(liveSeed + p * 13) % VALID_PROFILE_ICONS.length],
      solo, flex,
      history: {
        form: { games: 20, wins: formWins, kills: Math.round(bench.kills * 20), deaths: Math.round(bench.deaths * 20), assists: Math.round(bench.assists * 20) },
        onChampion: isSelf && champGames > 0
          ? { games: champGames, wins: champWins, kills: ownChampKills, deaths: ownChampDeaths, assists: ownChampAssists }
          : { games: champGames, wins: champWins, kills: Math.round(bench.kills * champGames), deaths: Math.round(bench.deaths * champGames), assists: Math.round(bench.assists * champGames) },
        mainRole, streak: p === 1 ? 4 : p === 6 ? -3 : 0, recent: [],
      },
      badges,
      damage: spec.damageClass,
      status: "ready",
      live: {
        level: liveLvl, kills, deaths, assists, cs: liveCs,
        wardScore: role === "support" ? 26 : role === "jungle" ? 13 : 9,
        items: itemsForDuration(spec, lm.gameTime, 4800, true, role),
        dead: p === 7,
      },
    };
  });

  const avgRankOf = (subset: typeof livePlayers) => {
    const avg = subset.reduce((s, pl) => s + toScore(pl.solo), 0) / subset.length;
    const pos = fromScore(avg);
    const phys = Math.round(100 * subset.filter(pl => pl.damage === "kPhysical").length / subset.length);
    return { averageTier: pos.tier, averageDivision: pos.division, ranked: subset.length, physical: phys };
  };
  const liveView = {
    gameId: 88001 + accIndex, queueId: 420, queue: "Ranked Solo/Duo", mapId: 11, gameTime: lm.gameTime,
    blue: avgRankOf(livePlayers.slice(0, 5)),
    red: avgRankOf(livePlayers.slice(5)),
    players: livePlayers,
    supported: true,
  };

  // --- Champ Select Draft (real 16.23 Lolalytics presets & matchup spells) ---
  const draftSpec = CHAMP_SPECS[lm.myChamp] ?? CHAMP_SPECS[266];
  const enemyChampId = lm.enemyChamps[myLiveIdx];
  const enemySpec = CHAMP_SPECS[enemyChampId] ?? CHAMP_SPECS[122];
  const draftPositions = ["top", "jungle", "middle", "bottom", "utility"];
  const draft: Draft = {
    allies: lm.allyChamps.map((cid, i) => ({ championId: i === 4 && i !== myLiveIdx ? 0 : cid, position: draftPositions[i], local: i === myLiveIdx, locked: i !== 4 || i === myLiveIdx })),
    enemies: lm.enemyChamps.map((cid, i) => ({ championId: i === myLiveIdx || i === 0 || i === 2 ? cid : 0, position: draftPositions[i], local: false, locked: i === myLiveIdx || i === 0 || i === 2 })),
    bans: [157, 777, 114, 254, 117, 84],
    allyBans: [157, 114, 117],
    enemyBans: [777, 254, 84],
    canLock: false,
    timer: { phase: "FINALIZATION", remainingMs: 38_000 },
  };

  const presets = [{
    index: 0,
    title: runeName(draftSpec.runes.keystone),
    ...draftSpec.runes,
    winPct: draftSpec.runeWinPct,
    play: draftSpec.runeGames,
    spells: draftSpec.spells,
  }];

  const runesView = {
    phase: "ChampSelect",
    championId: draftSpec.id,
    championName: draftSpec.name,
    position: cfg.role,
    mode: "ranked",
    source: "lolalytics",
    sourceLabel: "lolalytics",
    message: null,
    presets,
    trees: [],
    shards: [],
    applied: { name: `Swapper: ${draftSpec.name}`, championId: draftSpec.id, ...draftSpec.runes, presetIndex: 0 },
    autoApply: false,
    importItems: true,
    canApply: true,
    locked: true,
    spells: {
      spell1Id: draftSpec.spells[0],
      spell2Id: draftSpec.spells[1],
      available: [
        { id: 4, name: "Flash" }, { id: 12, name: "Teleport" }, { id: 14, name: "Ignite" },
        { id: 21, name: "Barrier" }, { id: 6, name: "Ghost" }, { id: 7, name: "Heal" }, { id: 3, name: "Exhaust" },
      ],
      applyWithRunes: true,
    },
    tier: "emerald_plus",
    tierLabel: "Emerald+",
    tiers: [],
    tierSupported: true,
    tierEmpty: false,
    enemyChampionId: enemySpec.id,
    enemyChampionName: enemySpec.name,
    enemies: [{ id: enemySpec.id, name: enemySpec.name }],
    enemySlots: 5,
    games: draftSpec.buildGames,
    stale: false,
    updatedAt: null,
  } as unknown as RunesView;

  return { config: cfg, snapshot, matches, liveView, draftView: { draft, runes: runesView }, playerDirectory };
}

/** Realistic Match Detail builder preserving exact invented Riot IDs, ranks, real 16.23 runes/spells/items/damage splits. */
export function buildRealisticMatchDetail(match: BuiltMatch): MatchView {
  const mins = Math.max(18, Math.round(match.duration / 60));
  const players = match.players.map((p, i) => {
    const spec = CHAMP_SPECS[p.championId] ?? CHAMP_SPECS[266];
    const [phys, mag, tru] = spec.damageSplit;
    const dmg = p.damage;
    const isJg = p.role === "jungle";
    const isSup = p.role === "support";
    const neutralCs = isJg ? Math.round(p.cs * 0.82) : isSup ? 0 : Math.min(p.cs, 6 + (i * 3) % 12);
    const ownJg = isJg ? Math.round(neutralCs * 0.84) : Math.round(neutralCs * 0.75);
    const wardsPlaced = isSup ? Math.round(p.vision * 0.56) : isJg ? Math.round(p.vision * 0.38) : Math.round(p.vision * 0.44);
    const wardsKilled = isSup ? Math.round(p.vision * 0.24) : isJg ? Math.round(p.vision * 0.26) : Math.max(1, Math.round(p.vision * 0.14));
    const ctrlWards = isSup ? Math.max(3, Math.round(mins / 6)) : isJg ? Math.max(2, Math.round(mins / 9)) : Math.max(1, Math.round(mins / 12));
    const perks = [p.runes.keystone, ...p.runes.primaryRunes, ...p.runes.secondaryRunes];
    const stats: Record<string, number | boolean> = {
      kills: p.kills, deaths: p.deaths, assists: p.assists, champLevel: p.level,
      largestKillingSpree: Math.min(p.kills, Math.max(0, Math.ceil(p.kills / Math.max(1, p.deaths)) + 1)),
      killingSprees: Math.min(3, Math.floor(p.kills / 3)),
      largestMultiKill: p.kills >= 9 ? 3 : p.kills >= 4 ? 2 : p.kills >= 1 ? 1 : 0,
      doubleKills: p.kills >= 5 ? Math.min(2, Math.floor(p.kills / 4)) : 0,
      tripleKills: p.kills >= 9 ? 1 : 0,
      quadraKills: 0, pentaKills: 0, unrealKills: 0,
      firstBloodKill: i === (match.win ? 1 : 6),
      firstBloodAssist: i === (match.win ? 0 : 5),
      totalTimeCrowdControlDealt: isSup ? 260 + mins * 6 : 95 + mins * 4,
      timeCCingOthers: isSup ? 34 + Math.round(mins * 0.6) : 14 + Math.round(mins * 0.35),
      longestTimeSpentLiving: Math.round(match.duration / Math.max(1.5, p.deaths * 0.75)),
      totalDamageDealtToChampions: dmg,
      physicalDamageDealtToChampions: Math.round(dmg * phys / 100),
      magicDamageDealtToChampions: Math.round(dmg * mag / 100),
      trueDamageDealtToChampions: Math.max(0, dmg - Math.round(dmg * phys / 100) - Math.round(dmg * mag / 100)),
      totalDamageDealt: Math.round(dmg * (isSup ? 2.4 : isJg ? 6.8 : 5.6)),
      physicalDamageDealt: Math.round(dmg * 5.2 * phys / 100),
      magicDamageDealt: Math.round(dmg * 5.2 * mag / 100),
      trueDamageDealt: Math.round(dmg * 5.2 * tru / 100),
      largestCriticalStrike: spec.role === "adc" ? 640 + p.level * 22 : 0,
      damageDealtToTurrets: isSup ? 680 + (p.ally === match.win ? 720 : 120) : Math.round((p.ally === match.win ? 4800 : 1650) * (p.role === "top" || p.role === "adc" ? 1.35 : 0.75)),
      damageDealtToObjectives: isJg ? Math.round(mins * 880) : isSup ? Math.round(mins * 130) : Math.round(mins * 360),
      totalDamageTaken: Math.round((isSup ? 15200 : p.role === "top" || isJg ? 24800 : 17600) * (mins / 28)),
      physicalDamageTaken: Math.round(13200 * (mins / 28)),
      magicalDamageTaken: Math.round(6900 * (mins / 28)),
      trueDamageTaken: Math.round(1600 * (mins / 28)),
      damageSelfMitigated: Math.round((p.role === "top" || isSup ? 21500 : 11400) * (mins / 28)),
      totalHeal: Math.round((p.role === "top" || isSup ? 8200 : 4100) * (mins / 28)),
      totalUnitsHealed: isSup ? 5 : 1,
      visionScore: p.vision,
      wardsPlaced, wardsKilled, visionWardsBoughtInGame: ctrlWards, sightWardsBoughtInGame: 0,
      goldEarned: p.gold, goldSpent: Math.max(0, p.gold - rangeInt(match.id + i * 19, 250, 950)),
      totalMinionsKilled: Math.max(0, p.cs - neutralCs),
      neutralMinionsKilled: neutralCs,
      neutralMinionsKilledTeamJungle: ownJg,
      neutralMinionsKilledEnemyJungle: Math.max(0, neutralCs - ownJg),
      turretKills: p.ally === match.win ? (p.role === "top" || p.role === "adc" ? 2 : 1) : (p.role === "top" ? 1 : 0),
      inhibitorKills: p.ally === match.win && i === 0 ? 1 : 0,
      firstTowerKill: i === (match.win ? 0 : 5),
      firstTowerAssist: i === (match.win ? 1 : 6),
      firstInhibitorKill: p.ally === match.win && i === 0,
      firstInhibitorAssist: p.ally === match.win && i === 2,
      win: p.ally ? match.win : !match.win,
      perkPrimaryStyle: p.runes.primaryPageId,
      perkSubStyle: p.runes.secondaryPageId,
    };
    perks.forEach((pid, idx) => { stats[`perk${idx}`] = pid; });
    p.items.forEach((itemId, idx) => { stats[`item${idx}`] = itemId; });

    return {
      participantId: i + 1,
      teamId: p.ally ? 100 : 200,
      championId: p.championId,
      riotId: p.riotId,
      me: p.me,
      role: p.role,
      level: p.level,
      spells: p.spells,
      perks,
      keystone: p.runes.keystone,
      secondaryStyle: p.runes.secondaryPageId,
      kills: p.kills,
      deaths: p.deaths,
      assists: p.assists,
      cs: p.cs,
      damage: p.damage,
      gold: p.gold,
      vision: p.vision,
      damageTaken: stats.totalDamageTaken as number,
      wardsPlaced,
      wardsKilled,
      items: p.items,
      placement: p.placement,
      score: p.score,
      rank: { tier: p.rank.tier, division: p.rank.division, lp: p.rank.lp, queue: match.queue === "flex" ? "Flex" as const : "Solo/Duo" as const },
      rankLoaded: true,
      stats,
    };
  });

  const blueGoldTotal = players.slice(0, 5).reduce((s, p) => s + p.gold, 0);
  const redGoldTotal = players.slice(5).reduce((s, p) => s + p.gold, 0);
  const finalDiff = blueGoldTotal - redGoldTotal;
  const totalGold = blueGoldTotal + redGoldTotal;

  const frames = Array.from({ length: mins + 1 }, (_, m) => {
    const t = m / mins;
    const goldDiff = m === 0 ? 0 : m === mins ? finalDiff : Math.round(Math.sin(t * Math.PI * 1.4) * (match.win ? -900 : 900) + finalDiff * (t ** 1.7));
    const blueGold = Math.round((5000 + (totalGold - 5000) * t + goldDiff) / 2);
    return { timestamp: Math.round(match.duration * 1000 * t), goldDiff, blueGold, redGold: blueGold - goldDiff };
  });

  const winTowers = mins >= 30 ? 9 : 8;
  const loseTowers = mins >= 30 ? 4 : 2;
  const winDrakes = mins >= 30 ? 4 : 3;
  const loseDrakes = mins >= 30 ? 2 : 1;
  const winBarons = mins >= 27 ? 1 : 0;
  const winInhibs = mins >= 28 ? 2 : 1;

  const makeObjectives = (won: boolean) => [
    { label: "Dragons", kills: won ? winDrakes : loseDrakes },
    { label: "Heralds", kills: won ? 1 : 0 },
    { label: "Grubs", kills: won ? 4 : 2 },
    { label: "Barons", kills: won ? winBarons : 0 },
    { label: "Towers", kills: won ? winTowers : loseTowers },
    { label: "Inhibitors", kills: won ? winInhibs : 0 },
  ];

  const winTeamId = match.win ? 100 : 200;
  const loseTeamId = match.win ? 200 : 100;
  const events: TimelineEvent[] = [
    { timestamp: Math.round(match.duration * 1000 * 0.24), teamId: loseTeamId, label: "Dragon" },
    { timestamp: Math.round(match.duration * 1000 * 0.42), teamId: winTeamId, label: "Tower" },
    { timestamp: Math.round(match.duration * 1000 * 0.50), teamId: winTeamId, label: "Dragon" },
    { timestamp: Math.round(match.duration * 1000 * 0.65), teamId: winTeamId, label: "Tower" },
    ...(winBarons ? [{ timestamp: Math.round(match.duration * 1000 * 0.80), teamId: winTeamId, label: "Baron" }] : []),
    { timestamp: Math.round(match.duration * 1000 * 0.88), teamId: winTeamId, label: "Inhibitor" },
    { timestamp: Math.round(match.duration * 1000 * 0.19), killerId: match.win ? 2 : 7, victimId: match.win ? 7 : 2, teamId: winTeamId, label: "Champion kill" },
    { timestamp: Math.round(match.duration * 1000 * 0.34), killerId: 1, victimId: 6, teamId: 100, label: "Champion kill" },
    { timestamp: Math.round(match.duration * 1000 * 0.56), killerId: 3, victimId: 8, teamId: 100, label: "Champion kill" },
    { timestamp: Math.round(match.duration * 1000 * 0.74), killerId: 9, victimId: 4, teamId: 200, label: "Champion kill" },
  ].sort((a, b) => a.timestamp - b.timestamp);

  return {
    game: match,
    players,
    teams: [
      { teamId: 100, win: match.win, objectives: makeObjectives(match.win), bans: match.bans[0] },
      { teamId: 200, win: !match.win, objectives: makeObjectives(!match.win), bans: match.bans[1] },
    ],
    timeline: { frames, events },
  };
}

function cleanDashes(text: string): string {
  return text.replace(/[–—]/g, "-");
}

function sanitizeGuide(g: MatchupGuide): MatchupGuide {
  const cleanTip = <T extends { text: string; heading?: string }>(t: T): T => ({
    ...t, text: cleanDashes(t.text), ...(t.heading ? { heading: cleanDashes(t.heading) } : {}),
  });
  return {
    ...g,
    favoredWhen: cleanDashes(g.favoredWhen),
    keyTips: g.keyTips.map(cleanTip),
    enemyTips: Object.fromEntries(Object.entries(g.enemyTips).map(([k, v]) => [k, v?.map(cleanTip)])),
    sections: Object.fromEntries(Object.entries(g.sections).map(([k, sec]) => [k, {
      ...sec,
      tips: sec.tips.map(cleanTip),
      ...(sec.timeline ? { timeline: sec.timeline.map(row => ({ at: cleanDashes(row.at), champion: cleanDashes(row.champion), enemy: cleanDashes(row.enemy) })) } : {}),
    }])),
    itemization: g.itemization.map(cleanTip),
  };
}

// Real shipped Ahri vs Zed guide, not the dev fixture: production builds hide guides marked `fixture`.
// The app's real shipped Ahri vs Zed guide, not the dev fixture (production builds hide guides marked `fixture`).
// Loaded on first use, so the guide file stays out of the demo's main chunk.
let shippedAhriVsZed: Promise<MatchupGuide> | null = null;
function loadAhriVsZed(): Promise<MatchupGuide> {
  // Vite serves JSON as a JS module, so the browser must import it without the `type: json` attribute;
  // Node (the website self-check) requires that attribute. `import.meta.env` only exists under Vite.
  shippedAhriVsZed ??= (import.meta.env
    ? import("../../../src-tauri/resources/guides/champions/103.json")
    : import("../../../src-tauri/resources/guides/champions/103.json", { with: { type: "json" } })
  ).then(({ default: file }) => {
    const guide = (file.guides as unknown as MatchupGuide[]).find(g => g.enemy === 238 && g.lane === "mid");
    if (!guide) throw new Error("103.json has no Ahri vs Zed mid guide");
    return sanitizeGuide(guide);
  });
  return shippedAhriVsZed;
}

// Invented VOD entries (fictional players/teams, non-playable fixture IDs, real 26.23 / 26.22 patch tags).
const DEMO_VODS: MatchupVod[] = [
  { videoId: "demo-vod-1", fixture: true, champion: 103, enemy: 238, lane: "mid", title: "Hold Charm to shut down Zed level 6 all-ins", player: "Miro", team: "VLT", rankClaim: "Challenger", patch: "26.23", result: "W", durationSec: 1746,
    moments: [{ atSec: 184, label: "Trades while Living Shadow is on cooldown", section: "early-lane" }, { atSec: 412, label: "Holds Charm through Death Mark", section: "overview" }, { atSec: 768, label: "Crashes wave before moving to river", section: "mid-lane" }, { atSec: 1431, label: "Saves final Spirit Rush charge for disengage", section: "win-condition" }] },
  { videoId: "demo-vod-2", fixture: true, champion: 103, enemy: 238, lane: "mid", title: "Mid priority into herald and bot dives vs Zed", player: "Taeyun", team: "KRN", rankClaim: "Grandmaster", patch: "26.23", result: "W", durationSec: 1582,
    moments: [{ atSec: 162, label: "Level 2 Electrocute trade", section: "early-lane" }, { atSec: 618, label: "Roams bot after cannon crash", section: "mid-lane" }, { atSec: 1210, label: "Flanks from fog around third drake", section: "win-condition" }] },
  { videoId: "demo-vod-3", fixture: true, champion: 103, enemy: 517, lane: "mid", title: "Spacing Sylas E2 and punishing melee waveclear", player: "Kaelen", team: "ARC", rankClaim: "Challenger", patch: "26.23", result: "W", durationSec: 1834,
    moments: [{ atSec: 205, label: "Punishes level 1 melee CS with autos", section: "early-lane" }, { atSec: 744, label: "Cancels Sylas engage with Charm", section: "mid-lane" }] },
  { videoId: "demo-vod-4", fixture: true, champion: 103, enemy: 112, lane: "mid", title: "Playing around Viktor E range before Lost Chapter", player: "Nori", team: "SOL", rankClaim: "Grandmaster", patch: "26.22", result: "L", durationSec: 1968,
    moments: [{ atSec: 240, label: "Steps outside minion wave to force Viktor choice", section: "early-lane" }, { atSec: 1090, label: "Catches Viktor on side wave rotation", section: "win-condition" }] },
  { videoId: "demo-vod-5", fixture: true, champion: 266, enemy: 122, lane: "top", title: "Kiting Darius Apprehend with Aatrox Q1-E spacing", player: "Haneul", team: "KRN", rankClaim: "Challenger", patch: "26.23", result: "W", durationSec: 1712,
    moments: [{ atSec: 195, label: "Baits Ghost and disengages with W slow", section: "early-lane" }, { atSec: 690, label: "Sweetspot Q2 cancels Darius Q heal", section: "mid-lane" }] },
  { videoId: "demo-vod-6", fixture: true, champion: 266, enemy: 114, lane: "top", title: "Delaying Q3 timing against Fiora Riposte", player: "Soren", team: "VLT", rankClaim: "Grandmaster", patch: "26.23", result: "W", durationSec: 1894,
    moments: [{ atSec: 230, label: "Holds Q3 until Riposte animation ends", section: "early-lane" }, { atSec: 940, label: "Flanks dragon fight with World Ender", section: "win-condition" }] },
  { videoId: "demo-vod-7", fixture: true, champion: 266, enemy: 516, lane: "top", title: "Punishing Ornn Brittle windows with Sundered Sky", player: "Dorian", team: "NVA", rankClaim: "Master", patch: "26.22", result: "W", durationSec: 1650,
    moments: [{ atSec: 310, label: "Freezes wave outside turret after crash", section: "early-lane" }] },
  { videoId: "demo-vod-8", fixture: true, champion: 222, enemy: 145, lane: "adc", title: "Rocket range harassment against Kai'Sa in lane", player: "Vexen", team: "ARC", rankClaim: "Challenger", patch: "26.23", result: "W", durationSec: 1824,
    moments: [{ atSec: 175, label: "Uses Fishbones splash on ranged creeps", section: "early-lane" }, { atSec: 1180, label: "Chains passive reset after baron contest", section: "win-condition" }] },
  { videoId: "demo-vod-9", fixture: true, champion: 222, enemy: 202, lane: "adc", title: "Managing wave state into Jhin fourth-shot pressure", player: "Miro", team: "VLT", rankClaim: "Grandmaster", patch: "26.23", result: "W", durationSec: 1940,
    moments: [{ atSec: 220, label: "All-ins while Jhin reloads", section: "early-lane" }, { atSec: 1320, label: "Front-to-back teamfight spacing", section: "win-condition" }] },
  { videoId: "demo-vod-10", fixture: true, champion: 164, enemy: 24, lane: "top", title: "Short Grasp trades around Jax Counter Strike", player: "Taeyun", team: "KRN", rankClaim: "Challenger", patch: "26.22", result: "W", durationSec: 1775,
    moments: [{ atSec: 210, label: "Uses wall-hug E to escape Counter Strike stun", section: "early-lane" }] },
];

// Game-weighted real lane average win rates from UGG_TIERLIST_16_23.
const LANE_AVG_WR: Record<Role, number> = Object.fromEntries(
  ROLES.map(r => {
    const rows = UGG_TIERLIST_16_23[r];
    const totGames = rows.reduce((s, row) => s + row[6], 0);
    const avg = totGames > 0 ? rows.reduce((s, row) => s + row[3] * row[6], 0) / totGames : 50;
    return [r, Number(avg.toFixed(2))];
  }),
) as Record<Role, number>;

export type RealisticDemoRuntime = {
  accounts: Account[];
  bundles: Map<string, BuiltAccountBundle>;
  createSharedHandlers: (getActiveId: () => string | null) => Record<string, (args: Record<string, any>) => unknown>;
};

export function createRealisticDemoRuntime(now = Date.now()): RealisticDemoRuntime {
  const bundles = new Map<string, BuiltAccountBundle>();
  REALISTIC_ACCOUNTS.forEach((cfg, idx) => {
    bundles.set(cfg.account.id, buildAccountBundle(cfg, idx, now));
  });

  const activeBundle = (id: string | null) =>
    (id && bundles.get(id)) || bundles.get(REALISTIC_ACCOUNTS[0].account.id)!;

  const namedItem = (id: number) => ({ id, name: ITEM_NAMES[id] ?? "Item" });
  const champNameOf = (id: number) => DD_CHAMPIONS[id]?.[0] ?? "Champion";
  const champKeyOf = (id: number) => DD_CHAMPIONS[id]?.[1] ?? "Aatrox";

  const formatRealBuild = (spec: ChampSpec) => ({
    starters: spec.starters.map(([id, count]) => ({ ...namedItem(id), count })),
    items: spec.items.map(namedItem),
    options: spec.options.map(([id, slot, wr, g]) => ({ ...namedItem(id), slot, winPct: wr, games: g })),
    games: spec.buildGames,
    coreGames: spec.coreGames,
    coreLen: spec.coreLen,
    coreWinPct: spec.coreWinPct,
    stale: false,
    updatedAt: null,
    skillPriority: spec.skillPriority,
    skillOrder: spec.skillOrder,
  });

  const formatAltBuild = (spec: ChampSpec) => {
    const alt = spec.altRunes!;
    return {
      starters: spec.starters.map(([id, count]) => ({ ...namedItem(id), count })),
      items: alt.items.map(namedItem),
      options: spec.options.map(([id, slot, wr, g]) => ({ ...namedItem(id), slot, winPct: wr, games: g })),
      games: alt.games,
      coreGames: alt.coreGames,
      coreLen: alt.coreLen,
      coreWinPct: alt.coreWinPct,
      stale: false,
      updatedAt: null,
      skillPriority: spec.skillPriority,
      skillOrder: spec.skillOrder,
    };
  };

  const createSharedHandlers = (getActiveId: () => string | null): Record<string, (a: Record<string, any>) => unknown> => {
    const cardsMap = () => Object.fromEntries(
      REALISTIC_ACCOUNTS.map((cfg) => {
        const b = bundles.get(cfg.account.id)!;
        const recent = b.matches.slice(0, 10);
        return [cfg.account.id, {
          rank: cfg.rank,
          wins: recent.filter(g => g.win).length,
          losses: recent.filter(g => !g.win).length,
          lastPlayed: b.matches[0]?.createdAt ?? now - 3600_000,
          updatedAt: now - 95_000,
        }];
      }),
    );

    const resolveRole = (pos: unknown, fallback: Role): Role =>
      typeof pos === "string" && pos in UGG_TIERLIST_16_23 ? (pos as Role) : fallback;

    return {
      get_current_patch: () => REAL_DATA_PATCH,
      newest_patch_notes: () => REAL_NOTES_PATCH,
      patch_banner: (a) => a.version === "26.22" ? RIOT_PATCH_NOTES_26_22.bannerUrl : RIOT_PATCH_NOTES_26_23.bannerUrl,
      get_latest_patch_notes: (a) => {
        if (a.version != null && a.version !== "26.23" && a.version !== "26.22") {
          throw new Error("Only current and previous patches are available");
        }
        const base = a.version === "26.22" ? RIOT_PATCH_NOTES_26_22 : RIOT_PATCH_NOTES_26_23;
        return { ...base, fetchedAt: now };
      },
      dashboard_snapshot: () => {
        const b = activeBundle(getActiveId());
        return { snapshot: b.snapshot, cards: cardsMap(), stale: false, message: null };
      },
      dashboard_benchmarks: (a) => {
        const picks = (a.picks ?? []) as [number, string][];
        const tierRaw = String(a.tier ?? "emerald").toLowerCase();
        const bracket = ["iron", "bronze", "silver", "gold", "platinum", "emerald", "diamond"].includes(tierRaw) ? tierRaw : "emerald";
        const out: Record<string, Bench> = {};
        for (const [id, role] of picks) {
          const cid = Number(id);
          const exact = LOLA_TIER_BENCHMARKS_16_23[`${bracket}|${cid}|${role}`];
          const fallback = CHAMP_SPECS[cid]?.bench ?? null;
          const bench = exact ?? fallback;
          if (bench) out[`${id}|${role}`] = bench;
        }
        return out;
      },
      full_app_match_detail: (a) => {
        const b = activeBundle(getActiveId());
        const match = b.matches.find(m => m.id === Number(a.gameId)) ?? b.matches[0];
        return buildRealisticMatchDetail(match);
      },
      full_app_search_players: (a) => {
        const b = activeBundle(getActiveId());
        const q = String(a.query ?? "").trim().toLowerCase();
        const matches = [...b.playerDirectory.values()]
          .filter(p => !q || p.riotId.toLowerCase().includes(q))
          .slice(0, 8)
          .map(p => ({
            riotId: p.riotId,
            icon: iconUrl(p.iconId),
            level: p.level,
            ally: p.riotId === b.config.duoPartner.riotId ? true : null,
            lastPlayed: b.matches.find(m => m.players.some(pl => pl.riotId === p.riotId))?.createdAt ?? null,
            live: false,
          }));
        return { connected: true, players: matches, message: q.includes("#") && !matches.length ? "No player found for that Riot ID." : null };
      },
      full_app_player_profile: (a) => {
        const b = activeBundle(getActiveId());
        const riotId = String(a.riotId ?? b.config.account.riotId);
        const key = riotId.toLowerCase();
        const entry = b.playerDirectory.get(key);
        const isSelf = key === b.config.account.riotId!.toLowerCase();
        const relevantGames = isSelf
          ? b.matches
          : b.matches
              .filter(m => m.players.some(p => p.riotId.toLowerCase() === key))
              .map(m => {
                const target = m.players.find(p => p.riotId.toLowerCase() === key)!;
                const mins = m.duration / 60;
                const team = m.players.filter(p => p.ally === target.ally);
                const tKills = team.reduce((s, p) => s + p.kills, 0);
                const tDmg = team.reduce((s, p) => s + p.damage, 0);
                const win = target.ally ? m.win : !m.win;
                return {
                  ...m,
                  championId: target.championId,
                  win,
                  kills: target.kills,
                  deaths: target.deaths,
                  assists: target.assists,
                  cs: target.cs,
                  level: target.level,
                  role: target.role,
                  items: target.items,
                  spells: target.spells,
                  keystone: target.runes.keystone,
                  placement: target.placement,
                  lpChange: null,
                  metrics: [
                    (target.kills + target.assists) / Math.max(1, target.deaths),
                    tKills ? Math.min(100, 100 * (target.kills + target.assists) / tKills) : null,
                    target.cs / mins,
                    target.damage / mins,
                    tDmg ? 100 * target.damage / tDmg : null,
                    target.vision / mins,
                  ],
                  players: m.players.map(p => ({ ...p, me: p.riotId.toLowerCase() === key, ally: p.ally === target.ally })),
                  opponent: m.players.findIndex(p => p.ally !== target.ally && p.role === target.role),
                };
              });
        const solo = entry?.rank ?? b.config.rank;
        const profileIcons = Object.fromEntries([...b.playerDirectory.entries()].map(([k, v]) => [k, iconUrl(v.iconId)]));
        return {
          region: entry?.region ?? b.config.account.region ?? "EUW",
          player: {
            riotId: entry?.riotId ?? riotId,
            icon: iconUrl(entry?.iconId ?? b.config.account.profileIconId ?? 5367),
            level: entry?.level ?? b.config.level,
            ally: isSelf || riotId === b.config.duoPartner.riotId,
            lastPlayed: relevantGames[0]?.createdAt ?? b.matches[0]?.createdAt ?? null,
          },
          solo: { ...solo, recordAvailable: true },
          flex: null,
          profileIcons,
          games: relevantGames.length ? relevantGames : b.matches.slice(0, 4),
          live: true,
          fetchedAt: now - 95_000,
          message: null,
        };
      },
      full_app_player_mmr: (a) => {
        const b = activeBundle(getActiveId());
        const key = String(a.riotId ?? "").toLowerCase();
        if (!key || key === b.config.account.riotId!.toLowerCase()) return b.config.mmr;
        const entry = b.playerDirectory.get(key);
        if (!entry) return b.config.mmr;
        const pos = fromScore(toScore(entry.rank) + 28);
        return { ...pos, games: 10, trend: 14 };
      },
      full_app_player_live: () => ({ live: false, view: null, message: null }),
      live_game_view: () => activeBundle(getActiveId()).liveView,
      live_game_live: () => activeBundle(getActiveId()).liveView,
      get_runes: () => {
        const params = new URLSearchParams(location.search);
        const b = activeBundle(getActiveId());
        if (params.has("draft-guide")) return bundles.get("vesper")!.draftView.runes;
        return b.draftView.runes;
      },
      full_app_draft: () => {
        const params = new URLSearchParams(location.search);
        const b = activeBundle(getActiveId());
        const draft = params.has("draft-guide") ? bundles.get("vesper")!.draftView.draft : b.draftView.draft;
        return { ...draft, supported: true };
      },
      get_tier_list: (a) => {
        const pos = resolveRole(a.position, "top");
        const rows = UGG_TIERLIST_16_23[pos].map(([id, tier, prevTier, winPct, pickPct, banPct, games]) => ({
          championId: id,
          name: champNameOf(id),
          tier,
          winPct,
          pickPct,
          banPct,
          games,
          prevTier,
        }));
        return { unavailable: false, stale: false, updatedAt: now - 1800_000, rows };
      },
      get_duo_list: () => ({
        rows: UGG_DUOS_16_23.map(([adcId, supportId, tier, winPct, games]) => ({
          adcId,
          adcName: champNameOf(adcId),
          supportId,
          supportName: champNameOf(supportId),
          tier,
          winPct,
          games,
        })),
        unavailable: false,
        stale: false,
        updatedAt: now - 1800_000,
      }),
      get_champion_list: () => Object.entries(DD_CHAMPIONS).map(([id, [name]]) => ({ id: Number(id), name })),
      get_champion_overview: (a) => {
        const cid = Number(a.championId);
        const spec = CHAMP_SPECS[cid];
        const role = resolveRole(a.position, spec?.role ?? "mid");
        const list = UGG_TIERLIST_16_23[role];
        const idx = list.findIndex(([id]) => id === cid);
        const entry = idx >= 0 ? list[idx] : null;
        if (!spec && !entry) {
          return { championId: cid, position: role, unavailable: true, damage: null, stats: null, build: null };
        }
        const laneAvg = LANE_AVG_WR[role];
        const stats = entry
          ? {
              tier: entry[1],
              rank: idx + 1,
              rankTotal: list.length,
              winPct: entry[3],
              avgWinPct: laneAvg,
              delta: Number((entry[3] - laneAvg).toFixed(2)),
              pickPct: entry[4],
              banPct: entry[5],
              games: entry[6],
              patch: REAL_DATA_PATCH,
            }
          : null;
        return {
          championId: cid,
          position: role,
          unavailable: false,
          damage: spec ? { physical: spec.damageSplit[0], magic: spec.damageSplit[1], trueDamage: spec.damageSplit[2] } : null,
          stats,
          build: spec
            ? {
                runes: spec.runes,
                spells: spec.spells,
                items: formatRealBuild(spec),
                skillOrder: spec.skillOrder,
              }
            : null,
        };
      },
      get_other_build: () => null,
      get_keystone_build: (a) => {
        const cid = Number(a.championId);
        const spec = CHAMP_SPECS[cid];
        if (!spec) return null;
        const requestedKs = Number(a.keystone ?? 0);
        const useAlt = Boolean(spec.altRunes && requestedKs && requestedKs === spec.altRunes.runes.keystone);
        const baseBuild = useAlt ? formatAltBuild(spec) : formatRealBuild(spec);
        if (!spec.altRunes) return { ...baseBuild, highestWin: null, highestWinMatchesPick: false };
        return {
          ...baseBuild,
          highestWin: {
            preset: {
              index: 1,
              title: spec.altRunes.title,
              ...spec.altRunes.runes,
              winPct: spec.altRunes.winPct,
              play: spec.altRunes.games,
              spells: spec.altRunes.spells,
            },
            build: formatAltBuild(spec),
          },
          highestWinMatchesPick: false,
        };
      },
      get_champion_counters: (a) => {
        const cid = Number(a.championId);
        const table = UGG_COUNTERS_16_23[cid];
        const role = resolveRole(a.position, table?.role ?? CHAMP_SPECS[cid]?.role ?? "mid");
        if (!table) {
          return { championId: cid, championName: champNameOf(cid), position: role, unavailable: true, stale: false, updatedAt: null, bestInto: [], beats: [] };
        }
        const toRow = ([id, winPct, delta, games]: [number, number, number, number]) => ({
          championId: id,
          name: champNameOf(id),
          winPct,
          delta,
          games,
          lowSample: games < 300,
        });
        return {
          championId: cid,
          championName: champNameOf(cid),
          position: role,
          unavailable: false,
          stale: false,
          updatedAt: now - 1800_000,
          bestInto: table.bestInto.map(toRow),
          beats: table.beats.map(toRow),
        };
      },
      full_app_matchups: (a) => {
        const cid = Number(a.championId);
        const map = UGG_MATCHUPS_16_23[cid];
        const role = resolveRole(a.position, CHAMP_SPECS[cid]?.role ?? "mid");
        if (!map) {
          return { championId: cid, championName: champNameOf(cid), position: role, bestInto: [], beats: [], unavailable: true, stale: false, updatedAt: null };
        }
        const beats = Object.entries(map)
          .map(([eidStr, [winPct, delta, games]]) => ({
            championId: Number(eidStr),
            name: champNameOf(Number(eidStr)),
            winPct,
            delta,
            games,
            lowSample: games < 300,
          }))
          .sort((x, y) => y.winPct - x.winPct);
        return {
          championId: cid,
          championName: champNameOf(cid),
          position: role,
          bestInto: [],
          beats,
          unavailable: false,
          stale: false,
          updatedAt: now - 1800_000,
        };
      },
      get_matchup: (a) => {
        const cid = Number(a.championId);
        const eid = Number(a.enemyChampionId);
        const featured = LOLA_FEATURED_MATCHUPS_16_23[`${cid}|${eid}`];
        if (featured) {
          return {
            enemyChampionId: eid,
            enemyName: champNameOf(eid),
            fallback: false,
            stats: {
              winPct: featured.winPct,
              avgWinPct: featured.avgWinPct,
              delta: featured.delta,
              games: featured.games,
              patch: REAL_DATA_PATCH,
            },
            preset: {
              index: 0,
              title: runeName(featured.runes.keystone),
              ...featured.runes,
              winPct: featured.runeWinPct,
              play: featured.runeGames,
              spells: featured.spells,
            },
            build: {
              starters: featured.starters.map(([id, count]) => ({ ...namedItem(id), count })),
              items: featured.items.map(namedItem),
              options: featured.options.map(([id, slot, wr, g]) => ({ ...namedItem(id), slot, winPct: wr, games: g })),
              games: featured.games,
              coreGames: featured.coreGames,
              coreLen: featured.coreLen,
              coreWinPct: featured.coreWinPct,
              stale: false,
              updatedAt: null,
              skillPriority: featured.skillPriority,
              skillOrder: featured.skillOrder,
            },
          };
        }
        const uggMu = UGG_MATCHUPS_16_23[cid]?.[String(eid)];
        const spec = CHAMP_SPECS[cid];
        if (!uggMu || !spec) return null;
        const [winPct, delta, games] = uggMu;
        const avgWinPct = Number((winPct - delta).toFixed(2));
        return {
          enemyChampionId: eid,
          enemyName: champNameOf(eid),
          fallback: true,
          stats: { winPct, avgWinPct, delta, games, patch: REAL_DATA_PATCH },
          preset: {
            index: 0,
            title: runeName(spec.runes.keystone),
            ...spec.runes,
            winPct: spec.runeWinPct,
            play: spec.runeGames,
            spells: spec.spells,
          },
          build: formatRealBuild(spec),
        };
      },
      get_matchup_guide: async (a) => {
        const cid = Number(a.championId);
        const eid = Number(a.enemyChampionId);
        const b = activeBundle(getActiveId());
        const played = b.matches.filter(m => m.championId === cid && m.players[m.opponent]?.championId === eid);
        const actualRecord = played.length
          ? { wins: played.filter(m => m.win).length, losses: played.filter(m => !m.win).length }
          : null;
        if (cid === 103 && eid === 238 && a.position === "mid") {
          return { guide: await loadAhriVsZed(), record: actualRecord ?? { wins: 6, losses: 4 } };
        }
        return { guide: null, record: actualRecord };
      },
      get_pro_builds: (a) => {
        const cid = Number(a.championId);
        const spec = CHAMP_SPECS[cid];
        if (!spec) {
          return { championId: cid, position: String(a.position ?? "mid"), role: String(a.position ?? "mid"), page: 1, hasMore: false, unavailable: true, message: null, stale: false, updatedAt: null, matches: [] };
        }
        const roleLabel = { top: "Top", jungle: "Jungle", mid: "Mid", adc: "Bot", support: "Support" }[spec.role];
        const page = Number(a.page ?? 1);
        const slice = INVENTED_PRO_HANDLES.slice((page - 1) * 4, page * 4);
        const agos = ["2h ago", "6h ago", "11h ago", "1d ago"];
        return {
          championId: spec.id,
          position: spec.role,
          role: spec.role,
          page,
          hasMore: page === 1,
          unavailable: false,
          message: null,
          stale: false,
          updatedAt: null,
          matches: slice.map((pro, i) => {
            const useAlt = Boolean(spec.altRunes && i === 1);
            const rSetup = useAlt ? spec.altRunes!.runes : spec.runes;
            const sp = useAlt ? spec.altRunes!.spells : spec.spells;
            const itemsList = useAlt ? spec.altRunes!.items : spec.items;
            return {
              matchId: spec.id * 100 + page * 10 + i,
              proName: pro.name,
              team: pro.team,
              league: pro.league,
              region: pro.region,
              role: spec.role,
              roleLabel,
              win: i !== 2,
              playedAgo: agos[i % agos.length],
              patch: REAL_DATA_PATCH,
              ...rSetup,
              kills: spec.role === "support" ? 1 + i % 2 : 5 + i * 2,
              deaths: 2 + i % 3,
              assists: spec.role === "support" ? 13 + i * 2 : 6 + i,
              spells: sp,
              finalItems: itemsList.map(namedItem),
              completedItems: spec.nonBoots.slice(0, 4).map(namedItem),
              skillOrder: spec.skillOrder.split(""),
              itemPath: [{ minute: 0, items: spec.starters.map(([id, count]) => ({ ...namedItem(id), count })) }],
              otp: Boolean(a.isOtp),
            };
          }),
        };
      },
      champion_abilities: (a) => {
        const cid = Number(a.championId);
        const ab = DD_ABILITIES_16_23[cid];
        if (!ab) throw new Error("Champion abilities unavailable");
        return {
          championId: cid,
          tooltipsReady: true,
          summoners: [],
          passive: {
            name: ab.passive.name,
            iconPath: `${DEMO_DDRAGON}/${DEMO_VERSION}/img/passive/${ab.passive.icon}`,
            description: ab.passive.description,
            tooltip: null,
          },
          spells: ab.spells.map(sp => ({
            key: sp.key,
            name: sp.name,
            iconPath: `${DEMO_DDRAGON}/${DEMO_VERSION}/img/spell/${sp.icon}`,
            description: sp.description,
            tooltip: null,
            cooldowns: sp.cooldowns,
            costs: sp.costs,
            ranges: sp.ranges,
            charges: 0,
            recharge: [],
          })),
        };
      },
      full_app_abilities: (a) => {
        const cid = Number(a.championId);
        const ab = DD_ABILITIES_16_23[cid];
        if (!ab) throw new Error("Champion abilities unavailable");
        return ab.spells.map(sp => ({
          key: sp.key,
          name: sp.name,
          icon: `${DEMO_DDRAGON}/${DEMO_VERSION}/img/spell/${sp.icon}`,
        }));
      },
      ability_icon: (a) => typeof a.path === "string" && a.path.startsWith(`${DEMO_DDRAGON}/`) ? a.path : null,
      get_all_vods: () => DEMO_VODS,
      get_champion_vods: (a) => DEMO_VODS.filter(v => v.champion === Number(a.championId)),
      get_matchup_vods: (a) => DEMO_VODS.filter(v => v.champion === Number(a.championId) && v.enemy === Number(a.enemyChampionId)),
      comps_team: () => {
        const b = activeBundle(getActiveId());
        const me = { puuid: `self-${b.config.account.id}`, riotId: b.config.account.riotId! };
        return {
          me,
          lobby: [],
          roster: b.config.squadRoster.map(m => ({ puuid: m.puuid, riotId: m.riotId, nickname: null, roles: [m.role === "adc" ? "bot" : m.role] })),
          suggested: [
            { puuid: b.config.squadRoster[0].puuid, riotId: b.config.squadRoster[0].riotId, games: 19 },
            { puuid: b.config.squadRoster[1].puuid, riotId: b.config.squadRoster[1].riotId, games: 11 },
            { puuid: b.config.squadRoster[2].puuid, riotId: b.config.squadRoster[2].riotId, games: 7 },
          ],
          overrides: { added: [], hidden: [] },
        };
      },
      comps_pools: (a) => {
        const b = activeBundle(getActiveId());
        const selfId = `self-${b.config.account.id}`;
        const selfRole = b.config.role === "adc" ? "bot" : b.config.role;
        const selfChamps = [...new Set(b.config.matchScripts.map(s => s.myChamp))].slice(0, 4);
        const byPuuid = new Map<string, unknown>();
        byPuuid.set(selfId, {
          puuid: selfId,
          error: null,
          profileIconId: b.config.account.profileIconId ?? 5367,
          entries: selfChamps.map((cid, i) => ({
            championId: cid, role: selfRole,
            reason: i < 2 ? `Played ${6 - i * 2} of last 20` : `Mastery ${140 - i * 20}k`,
            ...(i < 2 ? { games: 6 - i * 2 } : { masteryPoints: (140 - i * 20) * 1000, masteryLevel: 7 - i }),
          })),
        });
        for (const m of b.config.squadRoster) {
          byPuuid.set(m.puuid, {
            puuid: m.puuid,
            error: null,
            profileIconId: m.iconId,
            entries: m.champs.map((cid, i) => ({
              championId: cid, role: m.role === "adc" ? "bot" : m.role,
              reason: i === 0 ? "Played 7 of last 20" : `Mastery ${135 - i * 25}k`,
              ...(i === 0 ? { games: 7 } : { masteryPoints: (135 - i * 25) * 1000, masteryLevel: 6 - i }),
            })),
          });
        }
        return ((a.puuids ?? []) as string[]).map(id => byPuuid.get(id) ?? { puuid: id, entries: [], error: "Pool unavailable" });
      },
      full_app_item_names: () => ITEM_NAMES,
      champion_icon: (a) => `${DEMO_DDRAGON}/${DEMO_VERSION}/img/champion/${champKeyOf(Number(a.id))}.png`,
      champion_art: (a) => `${DEMO_DDRAGON}/img/champion/${a.kind === "splash" ? "splash" : a.kind === "loading" ? "loading" : "tiles"}/${champKeyOf(Number(a.id))}_0.jpg`,
      item_icon: (a) => `${DEMO_DDRAGON}/${DEMO_VERSION}/img/item/${Number(a.id)}.png`,
      spell_icon: (a) => {
        const map: Record<number, string> = {
          1: "SummonerBoost", 3: "SummonerExhaust", 4: "SummonerFlash", 6: "SummonerHaste",
          7: "SummonerHeal", 11: "SummonerSmite", 12: "SummonerTeleport", 14: "SummonerDot", 21: "SummonerBarrier",
        };
        return `${DEMO_DDRAGON}/${DEMO_VERSION}/img/spell/${map[Number(a.id)] ?? "SummonerFlash"}.png`;
      },
    };
  };

  return {
    accounts: REALISTIC_ACCOUNTS.map(c => c.account),
    bundles,
    createSharedHandlers,
  };
}
