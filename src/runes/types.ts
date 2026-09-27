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

/** The 6-item build people build with one preset's keystone, from lolalytics. */
export type KeystoneBuildView = {
  /** The build in order: core (3, often including boots) then slots 4-6. */
  items: ItemView[];
  /** The keystone's sample size, shown only when it is small. */
  games: number;
};

/** One recorded purchase in a pro's game. */
export type ItemOrderEntry = {
  itemId: number;
  name: string;
  /** Minute of the purchase, or -1 when the API did not report a path. */
  minute: number;
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
  canApply: boolean;
  locked: boolean;
  spells: SpellsView;
  tier: string;
  tierLabel: string;
  tiers: TierOption[];
  tierSupported: boolean;
  tierEmpty: boolean;
  games: number;
};

export type ProBuild = {
  matchId: number;
  proName: string;
  team: string;
  league: string;
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
  /** The pro's completed items in purchase order, with minute stamps. */
  itemOrder: ItemOrderEntry[];
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

/** A purchase-minute label for the pro item order, or "–" when unknown. */
export function fmtBuildMinute(minute: number): string {
  if (minute < 0) return "–";
  return `${minute}:00`;
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
