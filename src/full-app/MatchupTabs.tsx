import { useId } from "react";

const tabs = [["build", "Build"], ["guide", "Guide"]] as const;
export type MatchupTab = typeof tabs[number][0];

export function MatchupTabs({ value, onChange }: { value: MatchupTab; onChange: (tab: MatchupTab) => void }) {
  const id = useId();
  return <div className="mg-workspace-tabs" role="tablist" aria-label="Matchup views" onKeyDown={event => {
    if (event.defaultPrevented || event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
    if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
    const current = tabs.findIndex(([key]) => key === value);
    const next = event.key === "Home" ? 0 : event.key === "End" ? tabs.length - 1
      : (current + (event.key === "ArrowLeft" ? -1 : 1) + tabs.length) % tabs.length;
    event.preventDefault(); event.stopPropagation();
    onChange(tabs[next][0]);
    event.currentTarget.querySelectorAll<HTMLButtonElement>("button")[next]?.focus();
  }}>{tabs.map(([key, label]) => <button key={key} id={`${id}-${key}`} role="tab" aria-selected={value === key} aria-controls="matchup-view" tabIndex={value === key ? 0 : -1} onClick={() => onChange(key)}>{label}</button>)}</div>;
}
