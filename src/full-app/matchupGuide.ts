export const GUIDE_SECTIONS = [
  ["overview", "Overview"], ["early-lane", "Early lane"],
  ["mid-lane", "Mid lane"], ["win-condition", "Win condition"],
] as const;
export type GuideSectionId = typeof GUIDE_SECTIONS[number][0];
export type GuideTip = {
  heading?: string;
  /** Live base cooldown per rank: {cd:championId:Q|W|E|R}. */
  text: string;
  scope: "enemy" | "pair" | "champion";
  sourceIds?: string[];
  patch?: string;
  abilityKeys?: { champion: number; key: "P" | "Q" | "W" | "E" | "R" }[];
  itemIds?: number[];
  outdated?: string | true;
};
export type GuideSection = {
  tips: GuideTip[];
  alternatives?: { title: string; sourceId: string; tips: GuideTip[] }[];
  timeline?: { at: string; champion: string; enemy: string }[];
};
export type GuideSource = {
  id: string;
  author: string;
  rankClaim: string;
  url: string;
  kind: "mobafire" | "sheet" | "youtube" | "web";
};
// The Markdown compiler emits one flat JSON object per champion/enemy/lane.
export type MatchupGuide = {
  champion: number;
  enemy: number;
  lane: "top" | "jungle" | "mid" | "adc" | "support";
  patch: string;
  reviewed: boolean;
  fixture?: boolean;
  difficulty: "Easy" | "Medium" | "Hard" | "Skill matchup";
  favoredWhen: string;
  keyTips: [GuideTip, GuideTip, GuideTip];
  enemyTips: Partial<Record<GuideSectionId, GuideTip[]>>;
  sections: Record<GuideSectionId, GuideSection>;
  itemization: GuideTip[];
  sources: GuideSource[];
};
// An absent enemy distinguishes a general guide from a pair guide.
export type GeneralGuide = Omit<MatchupGuide, "enemy" | "difficulty" | "favoredWhen" | "enemyTips"> & {
  enemy?: never; difficulty?: never; favoredWhen?: never; enemyTips?: never;
};
export type AuthoredGuide = MatchupGuide | GeneralGuide;
// Player history belongs to the response, never the authored guide.
export type MatchupGuideResult = { guide: AuthoredGuide | null; record: { wins: number; losses: number } | null };

// Mirrors MOMENT_KINDS in the guides repo; the app drops kinds it does not know.
export type MomentKind = "kill" | "death" | "teamfight" | "dragon" | "baron" | "herald" | "grubs" | "tower" | "trade" | "roam" | "other";

export type MatchupVod = {
  videoId: string;
  title: string;
  player: string;
  playerImage?: string;
  team?: string;
  rankClaim: string;
  patch: string;
  result: "W" | "L";
  durationSec: number;
  moments: { atSec: number; label: string; section?: GuideSectionId; kind?: MomentKind }[];
  fixture?: boolean;
};

export function guideStep(index: number, key: string): number | null {
  const count = GUIDE_SECTIONS.length;
  if (key === "Home") return 0;
  if (key === "End") return count - 1;
  if (key === "ArrowLeft") return (index + count - 1) % count;
  if (key === "ArrowRight") return (index + 1) % count;
  return null;
}

export function itemChanges(usual: { id: number; name: string }[], matchup: { id: number; name: string }[]): string[] {
  return matchup.flatMap((item, slot) => {
    const before = usual.findIndex(old => old.id === item.id);
    if (before === slot) return [];
    return [before >= 0 ? `${item.name}: slot ${before + 1} → ${slot + 1}` : `${item.name} added in slot ${slot + 1}`];
  }).concat(usual.filter(item => !matchup.some(next => next.id === item.id)).map(item => `${item.name} omitted from matchup build`));
}
