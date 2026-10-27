import { perRank, type Ability } from "./cooldownModel.ts";

export type GuideTextPart = { text: string } | { champion: number; key: string };

export function parseGuideText(text: string): GuideTextPart[] {
  const parts: GuideTextPart[] = [];
  let end = 0;
  for (const match of text.matchAll(/\{cd:([1-9]\d*):([QWER])\}/g)) {
    if (!Number.isSafeInteger(Number(match[1]))) continue;
    if (match.index > end) parts.push({ text: text.slice(end, match.index) });
    parts.push({ champion: Number(match[1]), key: match[2] });
    end = match.index + match[0].length;
  }
  if (end < text.length) parts.push({ text: text.slice(end) });
  return parts;
}

export function liveCooldown(ability: Pick<Ability, "name" | "cooldowns" | "charges" | "recharge"> | undefined, fallbackName: string): string {
  const values = ability && ability.charges > 0 ? ability.recharge : ability?.cooldowns;
  if (!values?.length || !values.every(value => Number.isFinite(value) && value >= 0)) return ability?.name || fallbackName;
  const cooldown = perRank(values);
  return cooldown ? `${cooldown.split(" / ").join("/")}s` : ability?.name || fallbackName;
}
