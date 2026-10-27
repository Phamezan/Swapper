import type { Abilities } from "../cooldownModel";

// Preview-only substitute for champion_abilities; the real app uses live game data.
const rows: Record<number, [string, string, string, number[]][]> = {
  103: [
    ["Q", "Orb of Deception", "AhriQ", [7]], ["W", "Fox-Fire", "AhriW", [9, 8, 7, 6, 5]],
    ["E", "Charm", "AhriE", [12]], ["R", "Spirit Rush", "AhriR", [130, 105, 80]],
  ],
  238: [
    ["Q", "Razor Shuriken", "ZedQ", [6]], ["W", "Living Shadow", "ZedW", [20, 18.5, 17, 15.5, 14]],
    ["E", "Shadow Slash", "ZedE", [5, 4.5, 4, 3.5, 3]], ["R", "Death Mark", "ZedR", [120, 100, 80]],
  ],
};
export function guideAbilities(championId: number, cdn: string): Abilities | null {
  const abilities = rows[championId];
  if (!abilities) return null;
  return { championId, tooltipsReady: true, summoners: [],
    passive: { name: championId === 103 ? "Essence Theft" : "Contempt for the Weak", iconPath: null, description: "Placeholder passive description for the preview fixture." },
    spells: abilities.map(([key, name, icon, cooldowns]) => ({ key, name, iconPath: `${cdn}/img/spell/${icon}.png`, cooldowns, costs: [], ranges: [], charges: 0, recharge: [], description: `Fixture metadata for ${name}; cooldowns are before ability haste.` })),
  };
}
