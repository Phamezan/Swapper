import type { AuthoredGuide, GuideLookup, MatchupGuide } from "./matchupGuide";
import type { Abilities, Ability } from "./cooldownModel";

export type CardIdentity = { championId: number; enemyChampionId: number; lane: string; tier: string };
export type CardStats = {
  wins: number; games: number; patch: string; dataVersion: string; region: "world"; queue: "ranked_solo_5x5";
  tier: string; provider: "ugg"; endpointUrl: string; schemaId: string; fetchedAt: number; stale: boolean;
};
export type Gold15 = { mean: number; gamesAt15: number; signedTotal: number; units: "gold"; perspective: "champion"; schemaId: string };
export type CardKit = { sheet: Abilities; version: string; assetUrl: string; fetchedAt: number };
export type MatchupDataCard = CardIdentity & { kind: "data-based"; lane: MatchupGuide["lane"]; stats: CardStats; gold15?: Gold15; enemyKit?: CardKit };
export type GuideContent = { kind: "authored"; guide: AuthoredGuide } | MatchupDataCard;
export const STATS_MAX_AGE = 24 * 3600e3;
const number = (value: number) => value.toLocaleString("en-US", { maximumFractionDigits: 2 });
const laneName = (lane: string) => lane === "adc" ? "ADC" : lane;
const ranks = (champion: number, key: string) => champion === 77 ? 6 : key !== "R" ? 5 : [126,60,76,43].includes(champion) ? 4 : 3;
const positive = (values: number[], count?: number) => values.length > 0 && (count === undefined || values.length === count) && values.every(v => Number.isFinite(v) && v > 0);

export function usableSpell(spell: Ability, champion: number): boolean {
  const count = ranks(champion, spell.key);
  return ["Q", "W", "E", "R"].includes(spell.key) && !!spell.name.trim()
    && ((spell.charges > 0 && Number.isSafeInteger(spell.charges) && spell.charges <= 100)
      || positive(spell.charges > 0 ? spell.recharge : spell.cooldowns, count)
      || (positive(spell.ranges, count) && spell.ranges.every(r => r >= 100 && r < 25000)));
}

export function enemySpells(card: MatchupDataCard): Ability[] {
  const kit = card.enemyKit;
  if (!kit || kit.version !== card.stats.patch || kit.sheet.championId !== card.enemyChampionId) return [];
  return ["Q", "W", "E", "R"].flatMap(key => {
    const rows = kit.sheet.spells.filter(s => s.key === key);
    return rows.length === 1 && usableSpell(rows[0], card.enemyChampionId) ? rows : [];
  });
}

export function usableGold(card: MatchupDataCard): Gold15 | undefined {
  const g = card.gold15;
  return g && g.gamesAt15 >= 500 && Number.isSafeInteger(g.gamesAt15) && g.gamesAt15 <= card.stats.games
    && g.units === "gold" && g.perspective === "champion" && !!g.schemaId
    && Number.isFinite(g.mean) && Number.isFinite(g.signedTotal) && Math.abs(g.mean - g.signedTotal / g.gamesAt15) < .001 ? g : undefined;
}

export function dataCardEligible(card: MatchupDataCard | null, identity: CardIdentity, now = Date.now()): card is MatchupDataCard {
  if (!card || card.kind !== "data-based" || identity.championId <= 0 || identity.enemyChampionId <= 0 || identity.championId === identity.enemyChampionId
    || !["top", "jungle", "mid", "adc", "support"].includes(identity.lane)
    || card.championId !== identity.championId || card.enemyChampionId !== identity.enemyChampionId || card.lane !== identity.lane || card.tier !== identity.tier) return false;
  const s = card.stats;
  return !!s && s.provider === "ugg" && s.region === "world" && s.queue === "ranked_solo_5x5" && s.tier === identity.tier
    && /^\d+\.\d+$/.test(s.patch) && !!s.dataVersion && !!s.endpointUrl && !!s.schemaId
    && Number.isSafeInteger(s.games) && s.games >= 500 && Number.isSafeInteger(s.wins) && s.wins >= 0 && s.wins <= s.games
    && Number.isFinite(s.fetchedAt) && s.fetchedAt > 0 && now >= s.fetchedAt && now - s.fetchedAt <= STATS_MAX_AGE
    && (!!usableGold(card) || enemySpells(card).length >= 2);
}

export function selectGuideContent(guide: AuthoredGuide | null, lookup: GuideLookup, card: MatchupDataCard | null, identity: CardIdentity): GuideContent | null {
  if (identity.championId === identity.enemyChampionId) return null;
  const authored = guide?.champion === identity.championId && guide.lane === identity.lane
    && (guide.enemy === undefined || guide.enemy === identity.enemyChampionId) ? guide : null;
  if (authored?.enemy !== undefined) return { kind: "authored", guide: authored };
  if (lookup === "absent" && dataCardEligible(card, identity)) return card;
  return authored ? { kind: "authored", guide: authored } : null;
}

export function gameSentence(champion: string, enemy: string, lane: string, wins: number, games: number): string {
  return `${champion} won ${(100 * wins / games).toFixed(1)}% of ${number(games)} games against ${enemy} in ${laneName(lane)}.`;
}

export function goldSentence(champion: string, enemy: string, mean: number): string {
  const gold = Math.round(Math.abs(mean));
  return gold === 0 ? `${champion} was approximately even on gold at 15 minutes against ${enemy}.`
    : `${champion} averaged ${number(gold)} gold ${mean < 0 ? "behind" : "ahead of"} ${enemy} at 15 minutes.`;
}

function byRank(values: number[], unit: string): string {
  return values.every(v => v === values[0]) ? `${number(values[0])} ${unit} at every rank` : `${values.map(number).join("/")} ${unit} by rank`;
}

export function spellSentences(enemy: string, spell: Ability): string[] {
  const subject = `Enemy ${enemy} ${spell.key}`, lines: string[] = [];
  if (spell.charges > 0 && Number.isSafeInteger(spell.charges) && spell.charges <= 100) {
    lines.push(`${subject} holds ${spell.charges} charges${positive(spell.recharge) ? `, with a base recharge of ${byRank(spell.recharge, "seconds")}` : ""}.`);
  } else if (positive(spell.cooldowns)) lines.push(`${subject} has a base cooldown of ${byRank(spell.cooldowns, "seconds")}.`);
  if (positive(spell.ranges) && spell.ranges.every(r => r >= 100 && r < 25000)) lines.push(`${subject} has a listed ability range of ${byRank(spell.ranges, "units")}.`);
  return lines;
}

export function wilson(wins: number, games: number): [number, number] {
  const z2 = 1.96 ** 2, p = wins / games, denominator = 1 + z2 / games;
  const centre = (p + z2 / (2 * games)) / denominator;
  const margin = 1.96 * Math.sqrt(p * (1 - p) / games + z2 / (4 * games ** 2)) / denominator;
  return [wins === 0 ? 0 : Math.max(0, centre - margin), wins === games ? 1 : Math.min(1, centre + margin)];
}

/** Only presentation uses the season alias. Raw patch/cache identity stays 16.22. */
export function displayCardPatch(patch: string): string {
  const [major, minor] = patch.split(".");
  return `${Number(major) >= 16 ? Number(major) + 10 : major}.${minor}`;
}
