import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { RankCrest } from "./RankCrest";
import { fmtGames, type TierOption } from "./types";

type Props = {
  tiers: TierOption[];
  value: string;
  /** Games behind the current bracket, shown on the selected option only. */
  games: number;
  disabled: boolean;
  mode: "desktop" | "remote";
  onChange: (tier: string) => void;
};

function ChevronIcon() {
  return (
    <svg width={12} height={12} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2.4} strokeLinecap="round" strokeLinejoin="round" aria-hidden>
      <path d="m6 9 6 6 6-6" />
    </svg>
  );
}

function CheckIcon() {
  return (
    <svg className="rank-option-check" width={14} height={14} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2.6} strokeLinecap="round" strokeLinejoin="round" aria-hidden>
      <path d="M20 6 9 17l-5-5" />
    </svg>
  );
}

/** The rank-bracket picker. On the desktop flyout it drops a popover under the
 *  trigger; on the phone it is a bottom sheet, like the summoner-spell picker.
 *  Arrow keys and Escape work on both, and the options carry listbox roles. */
export function RankPicker({ tiers, value, games, disabled, mode, onChange }: Props) {
  const [open, setOpen] = useState(false);
  const selectedIndex = tiers.findIndex((option) => option.value === value);
  const [active, setActive] = useState(selectedIndex < 0 ? 0 : selectedIndex);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const optionRefs = useRef<(HTMLButtonElement | null)[]>([]);
  const current = tiers[selectedIndex] ?? tiers[0];

  useEffect(() => {
    if (open) optionRefs.current[active]?.focus();
    // Move focus to the active option whenever the picker opens or the active
    // row changes, so the listbox works from the keyboard.
  }, [open, active]);

  if (!current) return null;

  function openPicker() {
    setActive(selectedIndex < 0 ? 0 : selectedIndex);
    setOpen(true);
  }

  function close(returnFocus = true) {
    setOpen(false);
    if (returnFocus) triggerRef.current?.focus();
  }

  function choose(index: number) {
    const option = tiers[index];
    if (!option) return;
    close();
    if (option.value !== value) onChange(option.value);
  }

  function onListKeyDown(event: KeyboardEvent<HTMLElement>) {
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        setActive((index) => Math.min(index + 1, tiers.length - 1));
        break;
      case "ArrowUp":
        event.preventDefault();
        setActive((index) => Math.max(index - 1, 0));
        break;
      case "Home":
        event.preventDefault();
        setActive(0);
        break;
      case "End":
        event.preventDefault();
        setActive(tiers.length - 1);
        break;
      case "Enter":
      case " ":
        event.preventDefault();
        choose(active);
        break;
      case "Escape":
        event.preventDefault();
        close();
        break;
    }
  }

  function onTriggerKeyDown(event: KeyboardEvent<HTMLButtonElement>) {
    if (disabled) return;
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      openPicker();
    }
  }

  const options = tiers.map((option, index) => (
    <button
      key={option.value}
      ref={(element) => {
        optionRefs.current[index] = element;
      }}
      type="button"
      role="option"
      aria-selected={option.value === value}
      tabIndex={-1}
      className={`rank-option ${option.value === value ? "is-selected" : ""} ${
        mode === "desktop" && index === active ? "is-active" : ""
      }`}
      onMouseEnter={() => setActive(index)}
      onClick={() => choose(index)}
    >
      <RankCrest tier={option.value} mode={mode} size={22} />
      <span className="rank-option-label">{option.label}</span>
      {option.value === value && games > 0 && (
        <span className="rank-option-games">{fmtGames(games)} games</span>
      )}
      {option.value === value && <CheckIcon />}
    </button>
  ));

  return (
    <div className="rank-picker">
      <button
        ref={triggerRef}
        type="button"
        className="rank-trigger"
        disabled={disabled}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label={`Rank bracket: ${current.label}`}
        onClick={() => (open ? close() : openPicker())}
        onKeyDown={onTriggerKeyDown}
      >
        <RankCrest tier={current.value} mode={mode} size={22} />
        <span className="rank-trigger-label">{current.label}</span>
        <ChevronIcon />
      </button>

      {open && mode === "desktop" && (
        <>
          <button
            type="button"
            className="rank-backdrop"
            aria-label="Close the rank picker"
            tabIndex={-1}
            onClick={() => close(false)}
          />
          <div
            className="rank-popover"
            role="listbox"
            aria-label="Rank bracket"
            onKeyDown={onListKeyDown}
          >
            {options}
          </div>
        </>
      )}

      {open && mode === "remote" && (
        <div className="rank-sheet" role="dialog" aria-modal="true" aria-label="Choose a rank bracket">
          <div className="rank-sheet-body">
            <div className="rank-sheet-head">
              <strong>Rank bracket</strong>
              <button type="button" onClick={() => close()}>Close</button>
            </div>
            <div
              className="rank-sheet-list"
              role="listbox"
              aria-label="Rank bracket"
              onKeyDown={onListKeyDown}
            >
              {options}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
