import { AbilityHover, useAbilitySheet } from "./AbilityHover";
import { parseGuideText, liveCooldown } from "./guideText";
import type { GuideAbilities } from "./MatchupGuideContent";
import type { GuideTip } from "./matchupGuide";

export function GuideTipText({ text, abilities = {}, abilityKeys = [] }: { text: string; abilities?: GuideAbilities; abilityKeys?: GuideTip["abilityKeys"] }) {
  const names = abilityKeys.flatMap(ref => {
    const ability = abilities[ref.champion]?.find(ability => ability.key === ref.key);
    return ability?.name ? [{ ...ability, champion: ref.champion }] : [];
  });
  return <>{parseGuideText(text, names).map((part, i) => "text" in part ? part.text
    : "name" in part ? <span className="mg-inline-ability" key={i}>
      <AbilityHover championId={part.champion} abilityKey={part.key} icon={part.icon}>
        {part.icon && <img src={part.icon} alt="" />}<kbd>{part.key}</kbd>
      </AbilityHover>{" "}{part.name}
    </span>
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
