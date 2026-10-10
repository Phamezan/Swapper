export type Rune = {
  id: number;
  name: string;
  winPct: number | null;
  pickPct: number | null;
  play: number;
};

export type RuneRow = {
  kind: string;
  label: string;
  runes: Rune[];
};

export type RuneTree = {
  id: number;
  name: string;
  keystones: Rune[];
  rows: RuneRow[];
};

export type Preset = {
  index: number;
  title: string;
  primaryPageId: number;
  secondaryPageId: number;
  keystone: number;
  primaryRunes: number[];
  secondaryRunes: number[];
  shards: number[];
  winPct: number | null;
  play: number;
  /** op.gg's most-played spell pair `[D, F]`, empty when unknown. */
  spells: number[];
};

export type Applied = {
  name: string;
  championId: number;
  primaryPageId: number;
  secondaryPageId: number;
  keystone: number;
  primaryRunes: number[];
  secondaryRunes: number[];
  shards: number[];
  presetIndex: number | null;
  /** Set when the page came from the auto-apply setting, so the notice reads
   *  "Auto-applied …" rather than a manual apply. */
  autoApplied?: boolean;
};

export type TierOption = {
  value: string;
  label: string;
};

/** One item, with its display name. */
export type ItemView = {
  id: number;
  name: string;
};

/** One item stack in the recommended starting block. */
export type ItemStackView = ItemView & {
  count: number;
};

/** A reported later-slot alternative and its slot-level sample. */
export type ItemOptionView = ItemView & {
  slot: number;
  games: number;
  winPct: number | null;
  reason: string | null;
};

/** Starter, full build and slot alternatives for one preset's keystone. */
export type KeystoneBuildView = {
  starters: ItemStackView[];
  /** Core plus the most-supported unused choice for later slots. */
  items: ItemView[];
  options: ItemOptionView[];
  /** Sample size for the most-picked starter set. */
  games: number;
  coreGames: number;
  coreWinPct: number | null;
  /** True when this is the last successful build, served because the live
   *  source failed. */
  stale: boolean;
  /** Unix milliseconds of that last successful fetch, when known. */
  updatedAt: number | null;
  /** Ability max order, e.g. "QWE", when lolalytics reports one. */
  skillPriority?: string | null;
  /** Ability order for levels 1-15; matchup builds only. */
  skillOrder?: string | null;
};

/** Win rate and sample of one lane matchup. */
export type MatchupStats = {
  winPct: number;
  avgWinPct: number;
  /** Matchup win rate minus the champion's average. */
  delta: number;
  games: number;
  patch: string;
};

/** One champion in the Champion tab search. */
export type ChampionOption = {
  id: number;
  name: string;
};

/** One opposing champion in a champion's matchup table. */
export type CounterRow = {
  championId: number;
  name: string;
  /** The searched champion's win rate into this opponent. */
  winPct: number;
  /** `winPct` minus the opponent's overall win rate. */
  delta: number;
  games: number;
  /** Under 100 games: shown greyed out, below the rest. */
  lowSample: boolean;
};

/** One champion in a lane's tier list. */
export type TierRow = {
  championId: number;
  name: string;
  /** Our tier label: "S+", "S", "A", "B", "C" or "D". */
  tier: string;
  winPct: number;
  pickPct: number;
  banPct: number;
  games: number;
};

export type TierListView = {
  rows: TierRow[];
  unavailable: boolean;
  stale: boolean;
  updatedAt: number | null;
};

export type OverviewStats = {
  tier: string;
  winPct: number;
  avgWinPct: number;
  /** `winPct` minus the champion's average win rate. */
  delta: number;
  pickPct: number;
  banPct: number;
  games: number;
  rank: number;
  rankTotal: number;
  patch: string;
};

/** Share of damage by type, in percent. */
export type DamageSplit = {
  physical: number;
  magic: number;
  trueDamage: number;
};

/** What a champion probably builds; read-only. */
export type ChampionBuild = {
  runes: {
    keystone: number;
    primaryRunes: number[];
    secondaryRunes: number[];
    shards: number[];
  } | null;
  spells: number[];
  items: KeystoneBuildView;
  /** Ability order for levels 1-15. */
  skillOrder: string | null;
};

export type ChampionOverviewView = {
  championId: number;
  position: string;
  stats: OverviewStats | null;
  damage: DamageSplit | null;
  build: ChampionBuild | null;
  unavailable: boolean;
};

/** Everything the Champion tab loads. */
export type ChampionApi = {
  list: () => Promise<ChampionOption[]>;
  counters: (championId: number, position: string | null, tier: string) => Promise<ChampionCountersView>;
  overview: (championId: number, position: string | null, tier: string) => Promise<ChampionOverviewView>;
  tierList: (position: string, tier: string) => Promise<TierListView>;
};

export type ChampionCountersView = {
  championId: number;
  championName: string;
  /** The role the table is for ("mid", "adc"), or empty. */
  position: string;
  bestInto: CounterRow[];
  beats: CounterRow[];
  unavailable: boolean;
  stale: boolean;
  updatedAt: number | null;
};

/** The lane matchup against the enemy laner, from lolalytics. */
export type MatchupView = {
  enemyChampionId: number;
  enemyName: string;
  /** Null when lolalytics had no usable page for this matchup. */
  stats: MatchupStats | null;
  /** True when the generic build is shown: low sample or no matchup data. */
  fallback: boolean;
  preset: Preset | null;
  build: KeystoneBuildView | null;
};

/** One minute of a pro's item path: the items bought that minute, in purchase
 *  order, repeated ids stacked with a count. */
export type ItemPathGroup = {
  /** Whole minute of the purchases, or -1 when the API reported no path. */
  minute: number;
  items: ItemStackView[];
};

/** One selectable summoner spell. */
export type Spell = {
  id: number;
  name: string;
};

/** The player's spells and the options the picker may offer. */
export type SpellsView = {
  spell1Id: number;
  spell2Id: number;
  available: Spell[];
  applyWithRunes: boolean;
};

export type RunesView = {
  phase: string;
  championId: number;
  championName: string;
  position: string;
  mode: string;
  source: string;
  sourceLabel: string;
  message: string | null;
  presets: Preset[];
  trees: RuneTree[];
  shards: RuneRow[];
  applied: Applied | null;
  autoApply: boolean;
  /** Applying a preset also adds its item build to the League shop. */
  importItems: boolean;
  canApply: boolean;
  locked: boolean;
  spells: SpellsView;
  tier: string;
  tierLabel: string;
  tiers: TierOption[];
  tierSupported: boolean;
  tierEmpty: boolean;
  /** The revealed enemy in the player's lane; 0 when unknown. */
  enemyChampionId: number;
  enemyChampionName: string;
  /** Every revealed enemy, for the opponent picker. */
  enemies: { id: number; name: string }[];
  /** Enemy slots in the draft; unrevealed ones show as "TBD". */
  enemySlots: number;
  games: number;
  /** True when the presets are the last successful op.gg result. */
  stale: boolean;
  /** Unix milliseconds of that last successful fetch, when known. */
  updatedAt: number | null;
};

export type ProBuild = {
  matchId: number;
  proName: string;
  team: string;
  league: string;
  /** The league's region for the badge; the league code when unmapped. */
  region: string;
  /** API role slug (`top`, `jungle`, `mid`, `adc`, `supp`). */
  role: string;
  roleLabel: string;
  win: boolean;
  playedAgo: string;
  patch: string;
  keystone: number;
  primaryPageId: number;
  secondaryPageId: number;
  primaryRunes: number[];
  secondaryRunes: number[];
  shards: number[];
  kills: number;
  deaths: number;
  assists: number;
  /** The pro's spell pair `[D, F]`, empty when the API omitted it. */
  spells: number[];
  /** The final build as item icons, empty slots dropped and the trinket last. */
  finalItems: ItemView[];
  /** The items the pro completed, in the order the API lists them. */
  completedItems: ItemView[];
  /** Purchases grouped by whole minute, for the item path. */
  itemPath: ItemPathGroup[];
  /** The ability levelled at each level, one `"Q" | "W" | "E" | "R"` per level. */
  skillOrder: string[];
  /** True for a "One Trick Pony" entry, which has no real team. */
  otp: boolean;
};

export type ProBuildsView = {
  championId: number;
  position: string;
  role: string;
  page: number;
  hasMore: boolean;
  matches: ProBuild[];
  unavailable: boolean;
  message: string | null;
  /** True when these are the last successful games, served because the live
   *  source failed. */
  stale: boolean;
  /** Unix milliseconds of that last successful fetch, when known. */
  updatedAt: number | null;
};
/** The rank brackets op.gg accepts, kept in step with `runes::opgg::TIERS`. */
export const TIER_OPTIONS: TierOption[] = [
  { value: "all", label: "All ranks" },
  { value: "gold_plus", label: "Gold+" },
  { value: "platinum_plus", label: "Platinum+" },
  { value: "emerald_plus", label: "Emerald+" },
  { value: "diamond_plus", label: "Diamond+" },
  { value: "master_plus", label: "Master+" },
  { value: "challenger", label: "Challenger" },
];

/** The trinkets, which the final build draws last and smaller. Kept in step
 *  with `runes::probuilds::is_trinket`. */
const TRINKET_IDS = new Set([3340, 3363, 3364, 3330]);

export function isTrinketItem(id: number): boolean {
  return TRINKET_IDS.has(id);
}

/** A purchase-minute label for the pro item path, or "–" when unknown. */
export function fmtBuildMinute(minute: number): string {
  if (minute < 0) return "–";
  return `${minute} min`;
}

/** Whether a gameflow phase is part of, or around, a live game. Used to reset
 *  the Champion tab search once a game is over, without resetting while the
 *  player is in champion select or still in game. */
const GAME_PHASES = new Set([
  "GameStart",
  "InProgress",
  "Reconnect",
  "WaitingForStats",
  "PreEndOfGame",
  "EndOfGame",
]);

export function isGamePhase(phase: string): boolean {
  return GAME_PHASES.has(phase);
}

/** Tier badge colour class: `is-splus` for "S+", else `is-s`, `is-a`... from
 *  the letter (so lolalytics' "S-" or "A+" map to their letter). */
export function tierClass(label: string): string {
  return label === "S+" ? "is-splus" : `is-${label.charAt(0).toLowerCase()}`;
}

export function fmtPct(value: number | null): string {
  if (value === null || Number.isNaN(value)) return "–";
  return `${value.toFixed(1)}%`;
}

export function fmtGames(play: number): string {
  if (!play) return "–";
  if (play >= 1000) return `${(play / 1000).toFixed(1)}k`;
  return String(play);
}

/** A signed one-decimal difference, e.g. "+0.7". */
export function fmtDelta(value: number): string {
  return `${value >= 0 ? "+" : ""}${value.toFixed(1)}`;
}

/** A short "how long ago" label for a Unix-millisecond timestamp. */
export function fmtAgo(updatedAt: number | null | undefined): string {
  if (!updatedAt) return "unknown";
  const seconds = Math.max(0, Math.floor((Date.now() - updatedAt) / 1000));
  if (seconds < 60) return "just now";
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
  if (seconds < 86_400) return `${Math.floor(seconds / 3600)}h ago`;
  return `${Math.floor(seconds / 86_400)}d ago`;
}

export type Selection = {
  primaryPageId: number;
  secondaryPageId: number;
  keystone: number;
  primaryRunes: number[];
  secondaryRunes: number[];
  shards: number[];
};

export function selectionFromPreset(preset: Preset): Selection {
  return {
    primaryPageId: preset.primaryPageId,
    secondaryPageId: preset.secondaryPageId,
    keystone: preset.keystone,
    primaryRunes: [...preset.primaryRunes],
    secondaryRunes: [...preset.secondaryRunes],
    shards: [...preset.shards],
  };
}

export function selectionFromProBuild(build: ProBuild): Selection {
  return {
    primaryPageId: build.primaryPageId,
    secondaryPageId: build.secondaryPageId,
    keystone: build.keystone,
    primaryRunes: [...build.primaryRunes],
    secondaryRunes: [...build.secondaryRunes],
    shards: [...build.shards],
  };
}

export function selectionFromApplied(applied: Applied): Selection {
  return {
    primaryPageId: applied.primaryPageId,
    secondaryPageId: applied.secondaryPageId,
    keystone: applied.keystone,
    primaryRunes: [...applied.primaryRunes],
    secondaryRunes: [...applied.secondaryRunes],
    shards: [...applied.shards],
  };
}

export function sameSelection(a: Selection | null, b: Selection | null): boolean {
  if (!a || !b) return false;
  const list = (left: number[], right: number[]) =>
    left.length === right.length && left.every((value, index) => value === right[index]);
  return (
    a.primaryPageId === b.primaryPageId &&
    a.secondaryPageId === b.secondaryPageId &&
    a.keystone === b.keystone &&
    list(a.primaryRunes, b.primaryRunes) &&
    list(a.secondaryRunes, b.secondaryRunes) &&
    list(a.shards, b.shards)
  );
}

export function findRow(tree: RuneTree | undefined, runeId: number): number {
  if (!tree) return -1;
  return tree.rows.findIndex((row) => row.runes.some((rune) => rune.id === runeId));
}

/** League's primary rule: one rune per row, three rows. */
export function togglePrimary(selection: Selection, tree: RuneTree, runeId: number): Selection {
  if (selection.primaryRunes.includes(runeId)) {
    return { ...selection, primaryRunes: selection.primaryRunes.filter((id) => id !== runeId) };
  }
  const row = findRow(tree, runeId);
  const kept = selection.primaryRunes.filter((id) => findRow(tree, id) !== row);
  const next = [...kept, runeId].slice(-3);
  return { ...selection, primaryRunes: next };
}

/**
 * League's secondary rule: at most two runes, from different rows. Picking a
 * third rune replaces the one in the same row, or the oldest when the new rune
 * is in a fresh row.
 */
export function toggleSecondary(selection: Selection, tree: RuneTree, runeId: number): Selection {
  if (selection.secondaryRunes.includes(runeId)) {
    return {
      ...selection,
      secondaryRunes: selection.secondaryRunes.filter((id) => id !== runeId),
    };
  }
  const row = findRow(tree, runeId);
  const existing = selection.secondaryRunes.find((id) => findRow(tree, id) === row);
  if (existing !== undefined) {
    return {
      ...selection,
      secondaryRunes: selection.secondaryRunes.map((id) => (id === existing ? runeId : id)),
    };
  }
  const next =
    selection.secondaryRunes.length < 2
      ? [...selection.secondaryRunes, runeId]
      : [...selection.secondaryRunes.slice(1), runeId];
  return { ...selection, secondaryRunes: next };
}

export function selectShard(selection: Selection, rowIndex: number, runeId: number): Selection {
  const shards = [...selection.shards];
  while (shards.length < 3) shards.push(0);
  shards[rowIndex] = runeId;
  return { ...selection, shards };
}
