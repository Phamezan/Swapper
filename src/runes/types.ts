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
};

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
