import type { GuideSectionId, GuideTip } from "../matchupGuide";

// Shared "vs Zed" core, reusable by any champion's guide against Zed. Researched 2026-10-27.
export const zedCore: Partial<Record<GuideSectionId, GuideTip[]>> = {
  "early-lane": [{ scope: "enemy", text: "Zed often places Living Shadow before Death Mark so he can swap away from skillshots. Hold your crowd control until he has used the swap.", patch: "26.21", sourceIds: ["aikopanda"], abilityKeys: [{ champion: 238, key: "W" }, { champion: 238, key: "R" }] }],
  "win-condition": [{ scope: "enemy", text: "Death Mark always lands Zed behind his target. Pre-aim crowd control and escapes behind you, not toward where he cast it.", patch: "26.21", sourceIds: ["peng04"], abilityKeys: [{ champion: 238, key: "R" }] }],
};
