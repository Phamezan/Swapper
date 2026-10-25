/** Shapes from `champion_abilities` (src-tauri/src/full_app/abilities.rs). */
export type Ability = {
  key: string;
  name: string;
  iconPath: string | null;
  description?: string;
  tooltip?: string | null;
  cooldowns: number[];
  costs: number[];
  ranges: number[];
  charges: number;
  recharge: number[];
};
export type Summoner = { id: number; name: string; cooldown: number };
export type Abilities = {
  championId: number;
  passive: { name: string; iconPath: string | null; description?: string; tooltip?: string | null };
  spells: Ability[];
  summoners: Summoner[];
};

type TooltipPart = { text: string; kind?: "ranks" | "ap" | "ad" | "health" | "stack" };

/** Keep tooltip text intact; React renders these pieces as text, never HTML. */
export function tooltipParts(text: string): TooltipPart[] {
  const tokens = /\([+-][^()\n]*(?:\bAP\b|\bAD\b|\bHealth\b|per stack)[^()\n]*\)|\d+(?:\.\d+)?(?:[/–]\d+(?:\.\d+)?)+%?/g;
  const parts: TooltipPart[] = [];
  let end = 0;
  for (const match of text.matchAll(tokens)) {
    if (match.index > end) parts.push({ text: text.slice(end, match.index) });
    const value = match[0];
    const kind = value.includes("AP") ? "ap" : value.includes("AD") ? "ad" : value.includes("Health") ? "health" : value.includes("per stack") ? "stack" : "ranks";
    parts.push({ text: value, kind });
    end = match.index + value.length;
  }
  if (end < text.length) parts.push({ text: text.slice(end) });
  return parts;
}

const num = (value: number) => String(Math.round(value * 100) / 100);

/** One value per rank as "12 / 11 / 10", or a single value when every rank is
 *  the same. Empty when there is nothing to show (no values, or all zero). */
export function perRank(values: number[]): string {
  if (!values.length || values.every(v => v <= 0)) return "";
  return values.every(v => v === values[0]) ? num(values[0]) : values.map(num).join(" / ");
}

/** The secondary line of an ability: charges, cost and range, when known. */
export function abilityDetails(ability: Ability): string[] {
  const details: string[] = [];
  const recharge = perRank(ability.recharge);
  if (ability.charges > 0 && recharge) details.push(`${ability.charges} charges · ${recharge} s recharge`);
  const cost = perRank(ability.costs);
  if (cost) details.push(`Cost ${cost}`);
  const range = perRank(ability.ranges);
  if (range) details.push(`Range ${range}`);
  return details;
}
