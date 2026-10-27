import { AbilityHover, useAbilitySheet } from "./AbilityHover";
import { parseGuideText, liveCooldown } from "./guideText";
import type { GuideAbilities } from "./MatchupGuideContent";

export function GuideTipText({ text, abilities = {} }: { text: string; abilities?: GuideAbilities }) {
  return <>{parseGuideText(text).map((part, i) => "text" in part ? part.text
    : <LiveCooldown key={i} champion={part.champion} abilityKey={part.key} abilities={abilities} />)}</>;
}

function LiveCooldown({ champion, abilityKey, abilities }: { champion: number; abilityKey: string; abilities: GuideAbilities }) {
  const { sheet } = useAbilitySheet(champion, true);
  const ability = sheet?.spells.find(spell => spell.key === abilityKey);
  const metadata = abilities[champion]?.find(spell => spell.key === abilityKey);
  const name = ability?.name || metadata?.name || abilityKey;
  return <AbilityHover championId={champion} abilityKey={abilityKey} icon={metadata?.icon}>
    <span className="mg-live-cooldown" title={`${name} · base cooldown before ability haste`}>{liveCooldown(ability, name)}</span>
  </AbilityHover>;
}
