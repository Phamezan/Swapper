import { perRank, type Ability } from "./cooldownModel.ts";

export type GuideNamedAbility = { champion: number; key: string; name: string; icon: string | null };
export type GuideTextPart = { text: string } | { champion: number; key: string } | GuideNamedAbility;

export function parseGuideText(text: string, abilities: GuideNamedAbility[] = []): GuideTextPart[] {
  const parts: GuideTextPart[] = [];
  let end = 0;
  for (const match of text.matchAll(/\{cd:([1-9]\d*):([QWER])\}/g)) {
    if (!Number.isSafeInteger(Number(match[1]))) continue;
    if (match.index > end) parts.push({ text: text.slice(end, match.index) });
    parts.push({ champion: Number(match[1]), key: match[2] });
    end = match.index + match[0].length;
  }
  if (end < text.length) parts.push({ text: text.slice(end) });
  const names = [...abilities].filter(ability => ability.name).sort((a, b) => b.name.length - a.name.length);
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
