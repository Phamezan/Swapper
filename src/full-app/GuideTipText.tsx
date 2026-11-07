import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ItemIcon } from "../runes/ItemIcon";
import { RuneIcon } from "../runes/RuneIcon";
import { AbilityHover, useAbilitySheet } from "./AbilityHover";
import { liveCooldown, guideReferences, type GuideNamedReference, type GuideTextPart } from "./guideText";
import type { RuneCatalog } from "./BuildPanel";
import type { GuideAbilities } from "./MatchupGuideContent";
import type { GuideTip } from "./matchupGuide";
import { Tooltip } from "./ui/Tooltip";

let itemsRequest: Promise<Record<number, string>> | undefined;
let runesRequest: Promise<GuideNamedReference[]> | undefined;

export function useGuideReferences(abilities: GuideAbilities, abilityKeys: NonNullable<GuideTip["abilityKeys"]>, itemIds: number[]) {
  const [items, setItems] = useState<Record<number, string>>({});
  const [runes, setRunes] = useState<GuideNamedReference[]>([]);
  const hasItems = itemIds.length > 0;
  useEffect(() => {
    let live = true;
    if (hasItems) {
      itemsRequest ??= invoke<Record<number, string>>("full_app_item_names").catch(error => { itemsRequest = undefined; throw error; });
      void itemsRequest.then(value => { if (live) setItems(value); }).catch(() => {});
    }
    runesRequest ??= invoke<RuneCatalog>("full_app_rune_catalog").then(catalog => catalog.trees.flatMap(tree =>
      [tree, ...tree.keystones, ...tree.rows.flatMap(row => row.runes)].map(rune => ({ kind: "rune" as const, id: rune.id, name: rune.name }))
    )).catch(error => { runesRequest = undefined; throw error; });
    void runesRequest.then(value => { if (live) setRunes(value); }).catch(() => {});
    return () => { live = false; };
  }, [hasItems]);
  const names = abilityKeys.flatMap(ref => {
    const ability = abilities[ref.champion]?.find(ability => ability.key === ref.key);
    return ability?.name ? [{ ...ability, kind: "ability" as const, champion: ref.champion }] : [];
  });
  return guideReferences(names, itemIds.flatMap(id => items[id] ? [{ kind: "item" as const, id, name: items[id] }] : []), runes);
}

export function GuideTipText({ parts, abilities }: { parts: GuideTextPart[]; abilities: GuideAbilities }) {
  return <>{parts.map((part, i) => "text" in part ? part.text
    : "kind" in part ? part.kind === "ability" ? <span className="mg-inline-ability" key={i}>
      <AbilityHover championId={part.champion} abilityKey={part.key} icon={part.icon}>
        {part.icon && <img src={part.icon} alt="" />}<kbd>{part.key}</kbd>
      </AbilityHover>{" "}{part.name}
    </span>
    : <Tooltip key={i} label={part.name}><span className="mg-inline-reference" aria-label={part.name}>
      {part.kind === "item" ? <ItemIcon id={part.id} mode="desktop" /> : <RuneIcon id={part.id} mode="desktop" />}{part.name}
    </span></Tooltip>
    : <LiveCooldown key={i} champion={part.champion} abilityKey={part.key} abilities={abilities} />)}</>;
}

function LiveCooldown({ champion, abilityKey, abilities }: { champion: number; abilityKey: string; abilities: GuideAbilities }) {
  const { sheet } = useAbilitySheet(champion, true);
  const ability = sheet?.spells.find(spell => spell.key === abilityKey);
  const metadata = abilities[champion]?.find(spell => spell.key === abilityKey);
  const name = ability?.name || metadata?.name || abilityKey;
  return <AbilityHover championId={champion} abilityKey={abilityKey} icon={metadata?.icon}>
    <Tooltip label={`${name} · base cooldown before ability haste`}><span className="mg-live-cooldown" aria-label={`${name} · base cooldown before ability haste`}>{liveCooldown(ability, name)}</span></Tooltip>
  </AbilityHover>;
}
