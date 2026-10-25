import type { ReactNode } from "react";
import { useEffect, useState } from "react";
import { HoverCard } from "radix-ui";
import { invoke } from "@tauri-apps/api/core";
import { perRank, tooltipParts, type Abilities } from "./cooldownModel";
import "./ability-hover.css";

// One request per champion per session; every badge on the page shares it.
const sheets = new Map<number, Promise<Abilities | null>>();
function loadSheet(championId: number): Promise<Abilities | null> {
  if (!sheets.has(championId)) {
    sheets.set(championId, invoke<Abilities>("champion_abilities", { championId }).catch(() => {
      sheets.delete(championId);
      return null;
    }));
  }
  return sheets.get(championId)!;
}

/** Hovering (or focusing) `children` shows the ability's card: name, key, description, cooldown, cost and range per rank. */
export function AbilityHover({ championId, abilityKey, icon, children }: { championId: number; abilityKey: string; icon?: string | null; children: ReactNode }) {
  const [sheet, setSheet] = useState<Abilities | null>(null);
  const [open, setOpen] = useState(false);
  useEffect(() => {
    if (!open || sheet?.championId === championId) return;
    let live = true;
    void loadSheet(championId).then(found => { if (live) setSheet(found); });
    return () => { live = false; };
  }, [open, championId, sheet]);
  const ability = abilityKey === "P" ? null : sheet?.spells.find(spell => spell.key === abilityKey);
  const name = abilityKey === "P" ? sheet?.passive.name : ability?.name;
  const description = abilityKey === "P" ? sheet?.passive.description : ability?.description;
  const tooltip = abilityKey === "P" ? sheet?.passive.tooltip : ability?.tooltip;
  const text = tooltip || description;
  const rows: [string, string][] = ability ? ([
    ["Cooldown", perRank(ability.charges > 0 ? ability.recharge : ability.cooldowns) && `${perRank(ability.charges > 0 ? ability.recharge : ability.cooldowns)} s`],
    ["Charges", ability.charges > 0 ? String(ability.charges) : ""],
    ["Cost", perRank(ability.costs)],
    ["Range", perRank(ability.ranges)],
  ] as [string, string][]).filter(([, value]) => value) : [];
  return <HoverCard.Root open={open} onOpenChange={setOpen} openDelay={120} closeDelay={60}>
    <HoverCard.Trigger asChild>
      <span className="fa-ability-trigger" tabIndex={0} onFocus={() => setOpen(true)} onBlur={() => setOpen(false)} aria-label={name ? `${abilityKey}: ${name}` : abilityKey}>{children}</span>
    </HoverCard.Trigger>
    <HoverCard.Portal>
      <HoverCard.Content className="fa-ability-card" side="bottom" align="start" sideOffset={10} collisionPadding={12}>
        <header>
          {icon && <img src={icon} alt="" />}
          <div><strong>{name ?? (sheet ? abilityKey : "Loading…")}</strong><kbd>{abilityKey === "P" ? "Passive" : abilityKey}</kbd></div>
        </header>
        {text && <p className="fa-ability-text">{tooltipParts(text).map((part, i) => part.kind
          ? <span key={i} className={`fa-ability-${part.kind}`}>{part.text}</span>
          : part.text)}</p>}
        {rows.length > 0 && <dl>{rows.map(([label, value]) => <div key={label}><dt>{label}</dt><dd>{value}</dd></div>)}</dl>}
        {sheet === null && open && <p className="fa-ability-card-note">Ability details need League open once, or a moment to download.</p>}
      </HoverCard.Content>
    </HoverCard.Portal>
  </HoverCard.Root>;
}
