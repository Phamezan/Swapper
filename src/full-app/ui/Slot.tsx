import type { CSSProperties } from "react";
import { ChampionIcon } from "../../runes/ChampionIcon";
import { ItemIcon } from "../../runes/ItemIcon";
import { RuneIcon } from "../../runes/RuneIcon";
import { SpellIcon } from "../../runes/SpellIcon";
import "./ui.css";

export type SlotKind = "item" | "spell" | "rune" | "champion";

/** A framed icon square (round for runes). An id of 0 or less draws a subtle
 *  empty slot; `pending` draws the frame without loading the icon yet. */
export function Slot({ kind, id, size, label, pending, className }: {
  kind: SlotKind; id: number | null | undefined;
  /** Edge in px; omit to let page CSS set `--slot` (default 28px). */
  size?: number;
  /** Tooltip text (native title), e.g. the item name. */
  label?: string; pending?: boolean; className?: string;
}) {
  const empty = id == null || id <= 0;
  return <span className={`ui-slot is-${kind}${empty ? " is-empty" : ""}${className ? ` ${className}` : ""}`}
    style={size ? { "--slot": `${size}px` } as CSSProperties : undefined} title={empty ? undefined : label}>
    {!empty && !pending && (kind === "item" ? <ItemIcon id={id} name={label} mode="desktop" />
      : kind === "spell" ? <SpellIcon id={id} mode="desktop" />
      : kind === "rune" ? <RuneIcon id={id} mode="desktop" />
      : <ChampionIcon id={id} name={label} mode="desktop" />)}
  </span>;
}
