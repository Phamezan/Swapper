import { useState, type ReactNode } from "react";
import { Popover } from "radix-ui";
import { Check, ChevronDown } from "lucide-react";
import { SearchIcon } from "./ui/icons";
import { ChampionIcon } from "../runes/ChampionIcon";
import { RankCrest } from "../runes/RankCrest";
import { RoleIcon } from "../runes/RoleIcon";
import { TIER_OPTIONS, type ChampionOption } from "../runes/types";
import { NearViewport } from "./NearViewport";

export type PickOption = { value: string; label: string; icon?: ReactNode; disabled?: boolean };
export function Picker({ label, value, options, onChange, grid = false, searchable = false, disabled = false }: {
  label: string; value: string; options: PickOption[]; onChange: (value: string) => void;
  grid?: boolean; searchable?: boolean; disabled?: boolean;
}) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const selected = options.find(option => option.value === value);
  const visible = options.filter(option => option.label.toLowerCase().includes(query.trim().toLowerCase()));
  return <Popover.Root open={open} onOpenChange={next => { setOpen(next); setQuery(""); }}>
    <Popover.Trigger className="fa-picker" disabled={disabled} aria-label={`${label}: ${selected?.label ?? "Choose"}`}>
      {selected?.icon}<span>{selected?.label ?? label}</span><ChevronDown size={14} />
    </Popover.Trigger>
    <Popover.Portal><Popover.Content className={`fa-picker-popover${grid ? " is-grid" : ""}`} sideOffset={8} collisionPadding={12} aria-label={label} onKeyDown={event => {
      if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;
      const buttons = Array.from(event.currentTarget.querySelectorAll<HTMLButtonElement>(".fa-picker-option:not(:disabled)"));
      if (!buttons.length) return;
      event.preventDefault();
      const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
      const next = event.key === "Home" ? 0 : event.key === "End" ? buttons.length - 1 : (index + (event.key === "ArrowUp" ? -1 : 1) + buttons.length) % buttons.length;
      buttons[next].focus();
    }}>
      <p className="fa-picker-label">{label}</p>
      {searchable && <label className="fa-picker-search"><SearchIcon size={15} /><input aria-label={`Search ${label.toLowerCase()}`} placeholder="Search champions…" value={query} onChange={e => setQuery(e.target.value)} /></label>}
      <div className="fa-picker-options" role="group" aria-label={label}>{visible.map(option => <button type="button" className="fa-picker-option" key={option.value} disabled={option.disabled} aria-pressed={value === option.value} onClick={() => { onChange(option.value); setOpen(false); }}>
        {option.icon && <NearViewport className="fa-option-icon">{option.icon}</NearViewport>}<span>{option.label}</span>{value === option.value && <Check size={13} className="fa-picker-check" />}
      </button>)}</div>
      {!visible.length && <p className="fa-picker-label">No champions found.</p>}
    </Popover.Content></Popover.Portal>
  </Popover.Root>;
}

export const ROLES = [["top", "Top"], ["jungle", "Jungle"], ["mid", "Mid"], ["adc", "Bot"], ["support", "Support"]] as const;
export function RankPicker({ value, onChange, disabled }: { value: string; onChange: (value: string) => void; disabled?: boolean }) {
  return <Picker label="Rank" value={value} onChange={onChange} disabled={disabled} grid options={TIER_OPTIONS.map(option => ({ ...option, icon: <RankCrest tier={option.value} size={28} mode="desktop" /> }))} />;
}
export function RolePicker({ value, onChange }: { value: string; onChange: (value: string) => void }) {
  return <Picker label="Role" value={value} onChange={onChange} options={ROLES.map(([value, label]) => ({ value, label, icon: <RoleIcon role={value} size={20} mode="desktop" /> }))} />;
}
export function ChampionPicker({ label, value, champions, onChange }: { label: string; value: number; champions: ChampionOption[]; onChange: (value: number) => void }) {
  return <Picker label={label} value={String(value)} onChange={value => onChange(Number(value))} searchable options={[{ value: "0", label }, ...champions.map(champion => ({ value: String(champion.id), label: champion.name, icon: <ChampionIcon id={champion.id} name={champion.name} mode="desktop" /> }))]} />;
}
