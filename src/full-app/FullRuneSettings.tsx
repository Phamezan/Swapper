import type { ComponentProps } from "react";
import { Switch } from "../components/ui/switch";
import type { RuneSettings } from "../settings/RuneSettings";
import { RankPicker } from "./Picker";

export function FullRuneSettings(props: ComponentProps<typeof RuneSettings>) {
  return <>
    {[
      { label: "Auto-apply recommended runes", description: "Applies on lock-in.", checked: props.autoApply, change: props.onAutoApplyChange },
      { label: "Apply spells with runes", description: "Flash keeps your key.", checked: props.applySpells, change: props.onApplySpellsChange },
      { label: "Import item build", description: "Adds items to the League shop.", checked: props.importItems, change: props.onImportItemsChange },
    ].map(row => <div className="setting-row" key={row.label}><div className="setting-copy"><strong>{row.label}</strong><p>{row.description}</p></div><Switch aria-label={row.label} checked={row.checked} disabled={props.busy} onCheckedChange={row.change} /></div>)}
    <div className="setting-row"><div className="setting-copy"><strong>Build rank bracket</strong><p>Narrower brackets have fewer games.</p></div><RankPicker value={props.runeTier} onChange={props.onRuneTierChange} disabled={props.busy} /></div>
  </>;
}
