import type { ReactNode } from "react";
import { useEffect, useState } from "react";
import { HoverCard } from "radix-ui";
import { invoke } from "@tauri-apps/api/core";
import { perRank, tooltipParts, type Abilities } from "./cooldownModel";
import { AbilityIcon } from "./CooldownSheet";
import "./ability-hover.css";

// Share requests across badges; keep only sheets whose tooltip lookup completed.
const sheets = new Map<number, Promise<Abilities | null>>();
function loadSheet(championId: number): Promise<Abilities | null> {
  if (!sheets.has(championId)) {
    sheets.set(championId, invoke<Abilities>("champion_abilities", { championId }).then(sheet => {
      if (sheet.tooltipsReady === false) sheets.delete(championId);
      return sheet;
    }).catch(() => {
      sheets.delete(championId);
      return null;
    }));
  }
  return sheets.get(championId)!;
}

function useAbilitySheet(championId: number, open: boolean) {
  const [result, setResult] = useState<{ championId: number; sheet: Abilities | null } | null>(null);
  useEffect(() => {
    if (!open || championId <= 0) return;
    let live = true;
    let timer: ReturnType<typeof setTimeout>;
    const refresh = async () => {
      const sheet = await loadSheet(championId);
      if (!live) return;
      setResult({ championId, sheet });
      if (sheet?.tooltipsReady === false) timer = setTimeout(refresh, 1000);
    };
    void refresh();
    return () => { live = false; clearTimeout(timer); };
  }, [open, championId]);
  return { sheet: result?.championId === championId ? result.sheet : null, done: result?.championId === championId };
}

/** Hovering (or focusing) `children` shows the ability's card: name, key, description, cooldown, cost and range per rank. */
export function AbilityHover({ championId, abilityKey, icon, children }: { championId: number; abilityKey: string; icon?: string | null; children: ReactNode }) {
  const [open, setOpen] = useState(false);
  const { sheet } = useAbilitySheet(championId, open);
  const name = abilityName(sheet, abilityKey);
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
        <AbilityDetails sheet={sheet} abilityKey={abilityKey} />
        {sheet === null && open && <p className="fa-ability-card-note">Ability details need League open once, or a moment to download.</p>}
      </HoverCard.Content>
    </HoverCard.Portal>
  </HoverCard.Root>;
}

/** Hovering a champion (a draft card) shows its passive and Q/W/E/R in one card. `children` must accept a ref and props. */
export function ChampionAbilitiesHover({ championId, name, children }: { championId: number; name?: string; children: ReactNode }) {
  const [open, setOpen] = useState(false);
  const { sheet: loaded, done } = useAbilitySheet(championId, open);
  if (championId <= 0) return children;
  return <HoverCard.Root open={open} onOpenChange={setOpen} openDelay={250} closeDelay={80}>
    <HoverCard.Trigger asChild>{children}</HoverCard.Trigger>
    <HoverCard.Portal>
      <HoverCard.Content className="fa-ability-card is-champion" side="bottom" align="center" sideOffset={8} collisionPadding={12} aria-label={`${name ?? "Champion"} abilities`}>
        {!loaded ? <p className="fa-ability-card-note">{done ? "Ability details need League open once, or a moment to download." : "Loading abilities…"}</p>
          : <>{name && <h4>{name}</h4>}{[{ key: "P", iconPath: loaded.passive.iconPath }, ...loaded.spells].map(({ key, iconPath }) => <section key={key}>
            <header><AbilityIcon path={iconPath} letter={key} /><div><strong>{abilityName(loaded, key) ?? key}</strong><kbd>{key === "P" ? "Passive" : key}</kbd></div></header>
            <AbilityDetails sheet={loaded} abilityKey={key} />
          </section>)}</>}
      </HoverCard.Content>
    </HoverCard.Portal>
  </HoverCard.Root>;
}

function abilityName(sheet: Abilities | null, abilityKey: string) {
  return abilityKey === "P" ? sheet?.passive.name : sheet?.spells.find(spell => spell.key === abilityKey)?.name;
}

function AbilityDetails({ sheet, abilityKey }: { sheet: Abilities | null; abilityKey: string }) {
  const ability = abilityKey === "P" ? null : sheet?.spells.find(spell => spell.key === abilityKey);
  const text = abilityKey === "P" ? sheet?.passive.tooltip || sheet?.passive.description : ability?.tooltip || ability?.description;
  const rows: [string, string][] = ability ? ([
    ["Cooldown", perRank(ability.charges > 0 ? ability.recharge : ability.cooldowns) && `${perRank(ability.charges > 0 ? ability.recharge : ability.cooldowns)} s`],
    ["Charges", ability.charges > 0 ? String(ability.charges) : ""],
    ["Cost", perRank(ability.costs)],
    ["Range", perRank(ability.ranges)],
  ] as [string, string][]).filter(([, value]) => value) : [];
  return <>
    {text && <p className="fa-ability-text">{tooltipParts(text).map((part, i) => part.kind
      ? <span key={i} className={`fa-ability-${part.kind}`}>{part.text}</span>
      : part.text)}</p>}
    {rows.length > 0 && <dl>{rows.map(([label, value]) => <div key={label}><dt>{label}</dt><dd>{value}</dd></div>)}</dl>}
  </>;
}
