import { useState } from "react";
import { Info } from "lucide-react";
import { Switch } from "@/components/ui/switch";
import { TIER_OPTIONS } from "../runes/types";

type Props = {
  autoApply: boolean;
  onAutoApplyChange: (enabled: boolean) => void;
  applySpells: boolean;
  onApplySpellsChange: (enabled: boolean) => void;
  importItems: boolean;
  onImportItemsChange: (enabled: boolean) => void;
  runeTier: string;
  busy: boolean;
  onRuneTierChange: (tier: string) => void;
};

export function RuneSettings({
  autoApply,
  onAutoApplyChange,
  applySpells,
  onApplySpellsChange,
  importItems,
  onImportItemsChange,
  runeTier,
  busy,
  onRuneTierChange,
}: Props) {
  const [openInfo, setOpenInfo] = useState<string | null>(null);
  const toggle = (key: string) => setOpenInfo(openInfo === key ? null : key);

  return (
    <>
      <div className="setting-row">
        <div className="setting-copy">
          <label htmlFor="auto-runes-switch">Auto-apply recommended runes</label>
          <button className="info-button" aria-label="About Auto-apply recommended runes" aria-expanded={openInfo === "runes"} onClick={() => toggle("runes")}><Info size={13} /></button>
          {openInfo === "runes" && <p>Applies the recommended rune page when you lock in. Off by default.</p>}
        </div>
        <Switch id="auto-runes-switch" checked={autoApply} disabled={busy} onCheckedChange={onAutoApplyChange} aria-label="Auto-apply recommended runes" />
      </div>

      <div className="setting-row">
        <div className="setting-copy">
          <label htmlFor="spells-switch">Apply spells with runes</label>
          <button className="info-button" aria-label="About Apply summoner spells with runes" aria-expanded={openInfo === "spells"} onClick={() => toggle("spells")}><Info size={13} /></button>
          {openInfo === "spells" && <p>Sets recommended summoner spells with a preset or pro build, even with auto-apply off. Flash keeps your key.</p>}
        </div>
        <Switch id="spells-switch" checked={applySpells} disabled={busy} onCheckedChange={onApplySpellsChange} aria-label="Apply spells with runes" />
      </div>

      <div className="setting-row">
        <div className="setting-copy">
          <label htmlFor="items-switch">Import item build</label>
          <button className="info-button" aria-label="About Import item build" aria-expanded={openInfo === "items"} onClick={() => toggle("items")}><Info size={13} /></button>
          {openInfo === "items" && <p>Adds the preset's items to the League shop with runes, even with auto-apply off. Only one Swapper set is kept and removed after the game; your sets are never touched.</p>}
        </div>
        <Switch id="items-switch" checked={importItems} disabled={busy} onCheckedChange={onImportItemsChange} aria-label="Import item build" />
      </div>

      <div className="setting-row">
        <div className="setting-copy">
          <label htmlFor="rune-tier">Use builds from</label>
          <button className="info-button" aria-label="About the rune rank filter" aria-expanded={openInfo === "tier"} onClick={() => toggle("tier")}><Info size={13} /></button>
          {openInfo === "tier" && <p>Stats for this rank bracket from op.gg. Narrower brackets have fewer games.</p>}
        </div>
        <select
          id="rune-tier"
          className="setting-select"
          value={runeTier}
          disabled={busy}
          aria-label="Use builds from"
          onChange={(event) => onRuneTierChange(event.target.value)}
        >
          {TIER_OPTIONS.map((option) => (
            <option key={option.value} value={option.value}>{option.label}</option>
          ))}
        </select>
      </div>
    </>
  );
}
