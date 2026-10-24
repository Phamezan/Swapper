import type { ComponentProps } from "react";
import { Switch } from "../components/ui/switch";
import type { RuneSettings } from "../settings/RuneSettings";
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Picker, RankPicker } from "./Picker";
import type { FlashKey } from "./buildLogic";

/** Which key recommended spells put Flash on; saved on change. */
function FlashKeySetting({ disabled }: { disabled?: boolean }) {
  const [key, setKey] = useState<FlashKey | null>(null);
  useEffect(() => { void invoke<FlashKey>("get_flash_key").then(setKey).catch(() => setKey("keep")); }, []);
  return <div className="setting-row"><div className="setting-copy"><strong>Flash key</strong><p>Where recommended spells put Flash.</p></div>
    <Picker label="Flash key" value={key ?? "keep"} disabled={disabled || key === null} onChange={value => void invoke<FlashKey>("set_flash_key", { key: value }).then(setKey).catch(() => {})}
      options={[{ value: "keep", label: "Keep my key" }, { value: "d", label: "D" }, { value: "f", label: "F" }]} /></div>;
}

export function FullRuneSettings(props: ComponentProps<typeof RuneSettings>) {
  return <>
    {[
      { label: "Auto-apply recommended runes", description: "Applies on lock-in.", checked: props.autoApply, change: props.onAutoApplyChange },
      { label: "Apply spells with runes", description: "Sets the build's summoner spells too.", checked: props.applySpells, change: props.onApplySpellsChange },
      { label: "Import item build", description: "Adds items to the League shop.", checked: props.importItems, change: props.onImportItemsChange },
    ].map(row => <div className="setting-row" key={row.label}><div className="setting-copy"><strong>{row.label}</strong><p>{row.description}</p></div><Switch aria-label={row.label} checked={row.checked} disabled={props.busy} onCheckedChange={row.change} /></div>)}
    <FlashKeySetting disabled={props.busy} />
    <div className="setting-row"><div className="setting-copy"><strong>Build rank bracket</strong><p>Narrower brackets have fewer games.</p></div><RankPicker value={props.runeTier} onChange={props.onRuneTierChange} disabled={props.busy} /></div>
  </>;
}
