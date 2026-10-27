import type { Abilities } from "./cooldownModel";
import type { ItemView } from "../runes/types";

type RankCooldown = { rank: number; seconds: number };
export type CooldownRow = { key: string; name: string; iconPath: string | null; seconds: number; ranks: RankCooldown[]; recharge: boolean };

export function cooldownRows(sheet: Pick<Abilities, "spells"> | null): CooldownRow[] {
  return ["Q", "W", "E", "R"].flatMap(key => {
    const ability = sheet?.spells.find(spell => spell.key === key);
    if (!ability) return [];
    const recharge = ability.charges > 0;
    const values = recharge ? ability.recharge : ability.cooldowns;
    const seconds = values[0];
    if (!Number.isFinite(seconds) || seconds <= 0) return [];
    const ranks = Array.from({ length: key === "R" ? 3 : 5 }, (_, i) => ({ rank: i + 1, seconds: values[values.length === 1 ? 0 : i] }))
      .filter(value => Number.isFinite(value.seconds) && value.seconds > 0);
    return [{ key, name: ability.name || key, iconPath: ability.iconPath, seconds, ranks, recharge }];
  });
}

export function cooldownTicks(ranks: RankCooldown[], maximum: number) {
  const ticks: { ranks: number[]; seconds: number; percent: number }[] = [];
  for (const { rank, seconds } of ranks.filter(value => value.rank > 1)) {
    const same = ticks.find(tick => tick.seconds === seconds);
    if (same) same.ranks.push(rank);
    else ticks.push({ ranks: [rank], seconds, percent: seconds / maximum * 100 });
  }
  return ticks;
}

export function windowAbility(rows: CooldownRow[]): CooldownRow | null {
  return rows.filter(row => row.key !== "R").reduce<CooldownRow | null>((longest, row) => !longest || row.seconds > longest.seconds ? row : longest, null);
}

export type SpikeMarker = { kind: "level"; level: number; rank: number } | { kind: "item"; order: number; item: ItemView };

export function spikeMarkers(items: ItemView[]): SpikeMarker[] {
  const path = items.filter(item => Number.isInteger(item.id) && item.id > 0).slice(0, 3);
  if (!path.length) return [];
  return [
    ...[6, 11, 16].map((level, i): SpikeMarker => ({ kind: "level", level, rank: i + 1 })),
    ...path.map((item, i): SpikeMarker => ({ kind: "item", order: i + 1, item })),
  ];
}
