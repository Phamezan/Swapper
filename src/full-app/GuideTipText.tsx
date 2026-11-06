import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ItemIcon } from "../runes/ItemIcon";
import { RuneIcon } from "../runes/RuneIcon";
import { AbilityHover, useAbilitySheet } from "./AbilityHover";
import { parseGuideText, liveCooldown, guideReferences, type GuideNamedReference } from "./guideText";
import type { RuneCatalog } from "./BuildPanel";
import type { GuideAbilities } from "./MatchupGuideContent";
import type { GuideTip } from "./matchupGuide";

let itemsRequest: Promise<Record<number, string>> | undefined;
let runesRequest: Promise<GuideNamedReference[]> | undefined;

export function GuideTipText({ text, abilities = {}, abilityKeys = [], itemIds = [] }: { text: string; abilities?: GuideAbilities; abilityKeys?: GuideTip["abilityKeys"]; itemIds?: number[] }) {
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
  const references = guideReferences(names, itemIds.flatMap(id => items[id] ? [{ kind: "item" as const, id, name: items[id] }] : []), runes);
  return <>{parseGuideText(text, references).map((part, i) => "text" in part ? part.text
    : "kind" in part ? part.kind === "ability" ? <span className="mg-inline-ability" key={i}>
      <AbilityHover championId={part.champion} abilityKey={part.key} icon={part.icon}>
        {part.icon && <img src={part.icon} alt="" />}<kbd>{part.key}</kbd>
      </AbilityHover>{" "}{part.name}
    </span>
    : <span className="mg-inline-reference" key={i} title={part.name}>
      {part.kind === "item" ? <ItemIcon id={part.id} name={part.name} mode="desktop" /> : <RuneIcon id={part.id} mode="desktop" />}{part.name}
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
