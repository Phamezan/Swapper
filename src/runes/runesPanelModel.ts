import type { RunesView, Selection } from "./types";

/** Names the champion and role when there are no pro games, so it reads as
 *  "nobody plays this role" rather than a failed load. */
export function proEmptyMessage(championName: string, position: string, otp: boolean): string | undefined {
  const role = position && position !== "none" ? positionLabels[position] : undefined;
  if (!championName || !role) return undefined;
  const who = otp ? "one-trick" : "pro";
  return `No recent ${who} games for ${championName} as ${role} — try another role.`;
}

export const positionLabels: Record<string, string> = {
  top: "Top",
  jungle: "Jungle",
  mid: "Mid",
  adc: "Bot",
  support: "Support",
  none: "No role",
};

export const runeRoles = [
  { value: "top", label: "Top" },
  { value: "jungle", label: "Jungle" },
  { value: "mid", label: "Mid" },
  { value: "adc", label: "Bot" },
  { value: "support", label: "Support" },
] as const;

export const modeLabels: Record<string, string> = {
  ranked: "Ranked",
  aram: "ARAM",
  arena: "Arena",
};

export function defaultSelection(view: RunesView): Selection | null {
  const primary = view.trees[0];
  if (!primary || primary.keystones.length === 0) return null;
  const secondary = view.trees.find((tree) => tree.id !== primary.id);
  if (!secondary) return null;
  const primaryRunes = primary.rows
    .map((row) => row.runes[0]?.id)
    .filter((id): id is number => id !== undefined);
  const secondaryRunes = secondary.rows
    .map((row) => row.runes[0]?.id)
    .filter((id): id is number => id !== undefined)
    .slice(0, 2);
  return {
    primaryPageId: primary.id,
    secondaryPageId: secondary.id,
    keystone: primary.keystones[0].id,
    primaryRunes,
    secondaryRunes,
    shards: view.shards.map((row) => row.runes[0]?.id ?? 0),
  };
}

