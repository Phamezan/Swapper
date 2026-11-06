import { perRank, type Ability } from "./cooldownModel.ts";

export type GuideNamedAbility = { kind: "ability"; champion: number; key: string; name: string; icon: string | null };
export type GuideNamedReference = GuideNamedAbility | { kind: "item" | "rune"; id: number; name: string };
export type GuideTextPart = { text: string } | { champion: number; key: string } | GuideNamedReference;

// Tree names are ordinary prose rather than individual rune references.
export const IGNORED_RUNE_NAMES = new Set(["Precision", "Domination", "Resolve", "Inspiration", "Sorcery"]);

export function guideReferences(abilities: GuideNamedAbility[], items: GuideNamedReference[], runes: GuideNamedReference[]): GuideNamedReference[] {
  const named = [...abilities, ...items];
  return [...named, ...runes.flatMap(rune => {
    const alias = rune.name.match(/^[^:]+: ([A-Z].*)$/)?.[1];
    const names = alias ? [rune.name, alias] : [rune.name];
    if (IGNORED_RUNE_NAMES.has(rune.name) || named.some(ref => names.includes(ref.name))) return [];
    return names.map(name => ({ ...rune, name }));
  })];
}

export function parseGuideText(text: string, references: GuideNamedReference[] = []): GuideTextPart[] {
  const parts: GuideTextPart[] = [];
  let end = 0;
  for (const match of text.matchAll(/\{cd:([1-9]\d*):([QWER])\}/g)) {
    if (!Number.isSafeInteger(Number(match[1]))) continue;
    if (match.index > end) parts.push({ text: text.slice(end, match.index) });
    parts.push({ champion: Number(match[1]), key: match[2] });
    end = match.index + match[0].length;
  }
  if (end < text.length) parts.push({ text: text.slice(end) });
  const names = [...references].filter(ref => ref.name).sort((a, b) => b.name.length - a.name.length || (a.kind === "ability" ? 0 : 1) - (b.kind === "ability" ? 0 : 1));
  if (!names.length) return parts;
  const pattern = new RegExp(`(?<![\\p{L}\\p{N}_])(?:${names.map(ability => ability.name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")).join("|")})(?![\\p{L}\\p{N}_])`, "gu");
  return parts.flatMap(part => {
    if (!("text" in part)) return [part];
    const matched: GuideTextPart[] = [];
    let end = 0;
    for (const match of part.text.matchAll(pattern)) {
      if (match.index > end) matched.push({ text: part.text.slice(end, match.index) });
      matched.push(names.find(ability => ability.name === match[0])!);
      end = match.index + match[0].length;
    }
    if (end < part.text.length) matched.push({ text: part.text.slice(end) });
    return matched;
  });
}

export function liveCooldown(ability: Pick<Ability, "name" | "cooldowns" | "charges" | "recharge"> | undefined, fallbackName: string): string {
  const values = ability && ability.charges > 0 ? ability.recharge : ability?.cooldowns;
  if (!values?.length || !values.every(value => Number.isFinite(value) && value >= 0)) return ability?.name || fallbackName;
  const cooldown = perRank(values);
  return cooldown ? `${cooldown.split(" / ").join("/")}s` : ability?.name || fallbackName;
}
