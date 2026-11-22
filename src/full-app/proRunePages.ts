/** Pure pro rune-page logic: Leaguepedia rune/spell names -> CommunityDragon ids, page grouping. No IO. */

export type ProRunePage = {
  /** 9 ids in League order: keystone, 3 primary, 2 secondary, 3 shards. */
  runes: number[];
  primaryStyle: number;
  subStyle: number;
  /** Summoner spell ids in the order pros took them. */
  spells: number[];
  games: number;
  wins: number;
};

export type NameMaps = {
  runeIdByName: Map<string, number>;
  styleOfRune: Map<number, number>;
  shardRows: number[][];
  shardIdByName: Map<string, number>;
  spellIdByName: Map<string, number>;
};

export type PerkEntry = { id: number; name: string };
export type PerkStyles = { styles: { id: number; name: string; slots: { type: string; perks: number[] }[] }[] };
export type SpellEntry = { id: number; name: string; gameModes?: string[] };
export type ScoreboardRow = { Champion?: string; Role?: string; Runes?: string; SummonerSpells?: string; GameId?: string };

/** A runner-up page only shows when pros actually repeated it. */
export const RUNNER_UP_MIN_GAMES = 3;
/** Rust vault cap (MAX_PRESET_NAME in src-tauri/src/vault.rs). */
export const MAX_PRESET_NAME = 40;

/** Case/punctuation-insensitive match key: "Legend: Bloodline" == "legend bloodline". */
export const normalizeName = (s: string): string =>
  s.toLowerCase().replace(/[^a-z0-9]+/g, " ").trim().replace(/\s+/g, " ");

/** Build name lookups from CommunityDragon perks.json, perkstyles.json and summoner-spells.json. */
export function buildNameMaps(perks: PerkEntry[], perkStyles: PerkStyles, summonerSpells: SpellEntry[]): NameMaps {
  const shardRows: number[][] = [];
  const shardIds = new Set<number>();
  const styleOfRune = new Map<number, number>();
  for (const style of perkStyles.styles) {
    for (const slot of style.slots) {
      if (slot.type === "kStatMod") {
        slot.perks.forEach(id => shardIds.add(id));
      } else {
        slot.perks.forEach(id => styleOfRune.set(id, style.id));
      }
    }
    if (!shardRows.length) {
      const rows = style.slots.filter(s => s.type === "kStatMod").map(s => [...s.perks]);
      if (rows.length === 3) shardRows.push(...rows);
    }
  }
  const runeIdByName = new Map<string, number>();
  const shardIdByName = new Map<string, number>();
  for (const p of perks) {
    if (!p.name) continue;
    (shardIds.has(p.id) ? shardIdByName : runeIdByName).set(normalizeName(p.name), p.id);
  }
  // CDragon lists mode variants under the same name (JADE/CHERRY Flash, ...).
  // The CLASSIC spell wins; otherwise the smallest id does.
  const spellIdByName = new Map<string, number>();
  const spellClassic = new Map<string, boolean>();
  for (const s of summonerSpells) {
    if (!s.name) continue;
    const key = normalizeName(s.name);
    const classic = (s.gameModes ?? []).includes("CLASSIC");
    const prev = spellIdByName.get(key);
    if (prev === undefined || (classic && !spellClassic.get(key)) || (classic === spellClassic.get(key) && s.id < prev)) {
      spellIdByName.set(key, s.id);
      spellClassic.set(key, classic);
    }
  }
  return { runeIdByName, styleOfRune, shardRows, shardIdByName, spellIdByName };
}

export type MapResult =
  | { ok: true; runes: number[]; primaryStyle: number; subStyle: number; spells: number[] }
  | { ok: false; unmapped: string[] };

/**
 * Map one game's 9 rune names (+ spell names) to ids. A page is dropped whole
 * when any name is unknown, when a minor rune sits outside its tree, or when a
 * shard does not belong to its row.
 */
export function mapPage(runeNames: string[], spellNames: string[], maps: NameMaps): MapResult {
  const unmapped: string[] = [];
  if (runeNames.length !== 9) return { ok: false, unmapped: [...runeNames] };
  const runeId = (i: number): number | null => {
    const id = maps.runeIdByName.get(normalizeName(runeNames[i]));
    if (id === undefined) unmapped.push(runeNames[i]);
    return id ?? null;
  };
  const keystone = runeId(0);
  const primaryStyle = keystone !== null ? maps.styleOfRune.get(keystone) : undefined;
  if (keystone !== null && primaryStyle === undefined) unmapped.push(runeNames[0]);
  const primary = [runeId(1), runeId(2), runeId(3)];
  const secondary = [runeId(4), runeId(5)];
  let subStyle: number | undefined;
  secondary.forEach((id, i) => {
    if (id === null) return;
    const st = maps.styleOfRune.get(id);
    if (st === undefined || (subStyle !== undefined && st !== subStyle) || st === primaryStyle) unmapped.push(runeNames[4 + i]);
    else subStyle = st;
  });
  primary.forEach((id, i) => {
    if (id !== null && maps.styleOfRune.get(id) !== primaryStyle) unmapped.push(runeNames[1 + i]);
  });
  const shards: (number | null)[] = [6, 7, 8].map(i => {
    const id = maps.shardIdByName.get(normalizeName(runeNames[i]));
    if (id === undefined || !(maps.shardRows[i - 6]?.includes(id))) {
      unmapped.push(runeNames[i]);
      return null;
    }
    return id;
  });
  const spells: number[] = [];
  for (const name of spellNames) {
    const id = maps.spellIdByName.get(normalizeName(name));
    if (id === undefined) unmapped.push(name); else spells.push(id);
  }
  if (unmapped.length || keystone === null || primaryStyle === undefined || subStyle === undefined
    || primary.some(x => x === null) || secondary.some(x => x === null) || shards.some(x => x === null)) {
    return { ok: false, unmapped };
  }
  return {
    ok: true,
    runes: [keystone, ...(primary as number[]), ...(secondary as number[]), ...(shards as number[])],
    primaryStyle, subStyle, spells,
  };
}

const splitNames = (s: string | undefined): string[] => (s ?? "").split(",").map(x => x.trim()).filter(Boolean);

/** Group identical 9-rune pages (+ spells) into the top page and an optional runner-up. */
export function groupPages(
  played: { runes: number[]; primaryStyle: number; subStyle: number; spells: number[]; win: boolean }[],
): ProRunePage[] {
  const groups = new Map<string, ProRunePage>();
  for (const p of played) {
    const key = `${p.runes.join(",")}|${[...p.spells].sort((a, b) => a - b).join(",")}`;
    const g = groups.get(key);
    if (g) { g.games++; if (p.win) g.wins++; }
    else groups.set(key, { runes: [...p.runes], primaryStyle: p.primaryStyle, subStyle: p.subStyle, spells: [...p.spells], games: 1, wins: p.win ? 1 : 0 });
  }
  const sorted = [...groups.values()].sort((a, b) => b.games - a.games || b.wins - a.wins || a.runes.join(",").localeCompare(b.runes.join(",")));
  if (!sorted.length) return [];
  const runner = sorted[1];
  return runner && runner.games >= RUNNER_UP_MIN_GAMES ? [sorted[0], runner] : [sorted[0]];
}

export type RuneCoverage = { total: number; withRunes: number; missing: number; dropped: number; unmapped: string[] };

/**
 * The answer champion's page in each of its pro games. For draft entries (and
 * bot/support pairs) the first answer champion carries the page. Games with no
 * runes recorded count as missing; games whose names fail to map count as
 * dropped and their names are reported once.
 */
export function runePagesForAnswer(
  answer: { answer: string[]; proGames: { gameId: string; result: string }[] },
  rowsByGame: Map<string, ScoreboardRow[]>,
  maps: NameMaps,
): { pages: ProRunePage[]; coverage: RuneCoverage } {
  const champ = normalizeName(answer.answer[0] ?? "");
  const coverage: RuneCoverage = { total: answer.proGames.length, withRunes: 0, missing: 0, dropped: 0, unmapped: [] };
  const played: { runes: number[]; primaryStyle: number; subStyle: number; spells: number[]; win: boolean }[] = [];
  for (const g of answer.proGames) {
    const row = (rowsByGame.get(g.gameId) ?? []).find(r => normalizeName(r.Champion ?? "") === champ);
    if (!row || !splitNames(row.Runes).length) { coverage.missing++; continue; }
    const mapped = mapPage(splitNames(row.Runes), splitNames(row.SummonerSpells), maps);
    if (!mapped.ok) {
      coverage.dropped++;
      for (const name of mapped.unmapped) if (!coverage.unmapped.includes(name)) coverage.unmapped.push(name);
      continue;
    }
    coverage.withRunes++;
    played.push({ runes: mapped.runes, primaryStyle: mapped.primaryStyle, subStyle: mapped.subStyle, spells: mapped.spells, win: g.result === "Win" });
  }
  return { pages: groupPages(played), coverage };
}

/** Tauri `save_rune_preset` selection (serde camelCase) for one pro page. */
export function presetSelection(page: ProRunePage): {
  primaryPageId: number; secondaryPageId: number; keystone: number;
  primaryRunes: number[]; secondaryRunes: number[]; shards: number[];
} {
  return {
    primaryPageId: page.primaryStyle,
    secondaryPageId: page.subStyle,
    keystone: page.runes[0],
    primaryRunes: page.runes.slice(1, 4),
    secondaryRunes: page.runes.slice(4, 6),
    shards: page.runes.slice(6, 9),
  };
}

/** Short preset name within the vault's 40-char cap, e.g. "Pro vs Jhin". */
export const presetName = (threat: string): string => `Pro vs ${threat}`.slice(0, MAX_PRESET_NAME);
