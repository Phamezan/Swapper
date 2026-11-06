import { useEffect, useId, useRef, useState } from "react";
import { ChevronDown, ChevronLeft, ChevronRight, TriangleAlert } from "lucide-react";
import type { ChampionOption, MatchupView } from "../runes/types";
import { Skeleton } from "./ui/States";
import { GuideSectionContent } from "./MatchupGuideContent";
import { MatchupGuideSummary } from "./MatchupGuideSummary";
import { GUIDE_SECTIONS, guideStep, type GuideSectionId } from "./matchupGuide";
import type { useMatchupGuide } from "./useMatchupGuide";
import { TradeWindows } from "./TradeWindows";
import { PowerSpikeTimeline } from "./PowerSpikeTimeline";
import "./matchup-guide.css";

export function MatchupGuideCarousel({ guide, loading, abilities, champion, enemy, lane, tier, matchup, initialSection }: Pick<ReturnType<typeof useMatchupGuide>, "guide" | "loading" | "abilities"> & {
  champion: ChampionOption; enemy: ChampionOption; lane: string; tier: string; matchup: MatchupView | null;
  initialSection?: GuideSectionId;
}) {
  const [collapsed, setCollapsed] = useState(() => {
    try { return localStorage.getItem("swapper.guideHidden") === "1"; } catch { return false; }
  });
  const toggle = () => {
    setCollapsed(!collapsed);
    try { localStorage.setItem("swapper.guideHidden", collapsed ? "0" : "1"); } catch { /* preference only */ }
  };
  const [index, setIndex] = useState(() => Math.max(0, GUIDE_SECTIONS.findIndex(([section]) => section === initialSection)));
  const tabs = useRef<HTMLDivElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  const id = useId();
  useEffect(() => { if (initialSection) tabs.current?.querySelector<HTMLButtonElement>('[aria-selected="true"]')?.focus({ preventScroll: true }); }, [initialSection]);
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (collapsed || event.defaultPrevented || event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
      const target = event.target instanceof HTMLElement ? event.target : null;
      if (target?.closest('input, textarea, select, [contenteditable="true"], [role="slider"], [role="combobox"], [data-radix-popper-content-wrapper]')) return;
      const otherTabs = target?.closest('[role="tablist"]');
      if (otherTabs && otherTabs !== tabs.current) return;
      const next = guideStep(index, event.key);
      if (next === null) return;
      event.preventDefault(); setIndex(next);
      if (tabs.current?.contains(target) || panel.current?.contains(target)) tabs.current?.querySelectorAll<HTMLButtonElement>('[role="tab"]')[next]?.focus();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [index, collapsed]);
  useEffect(() => { tabs.current?.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: "nearest", inline: "nearest" }); }, [index]);
  const count = GUIDE_SECTIONS.length;
  return <section className={`mg-deck${collapsed ? " is-collapsed" : ""}`} aria-label="Matchup guide">
    <header className="mg-card-header"><h2>Guide</h2>
      <button type="button" className="mg-visual-toggle" aria-label={`${collapsed ? "Expand" : "Collapse"} guide`} aria-expanded={!collapsed} aria-controls={`${id}-content`} onClick={toggle}><ChevronDown size={16} aria-hidden /></button>
    </header>
    <div id={`${id}-content`} className="mg-deck-content" hidden={collapsed}>
    {guide && guide.enemy === undefined && <div className="ui-notice is-warn" role="status">
      <TriangleAlert size={16} aria-hidden /><span>No {champion.name} vs {enemy.name} guide yet. Showing the general {champion.name} guide, so tips aren't specific to this matchup.</span>
    </div>}
    <div ref={tabs} className="mg-tabs" role="tablist" aria-label="Guide sections">{GUIDE_SECTIONS.map(([key, label], i) =>
      <button key={key} id={`${id}-tab-${key}`} role="tab" aria-selected={i === index} aria-controls={`${id}-panel-${key}`} tabIndex={i === index ? 0 : -1} onClick={() => setIndex(i)}>{label}</button>)}
    </div>
    <div className="mg-stage" ref={panel}>{GUIDE_SECTIONS.map(([key, label], i) => <div key={key} id={`${id}-panel-${key}`} role="tabpanel" aria-labelledby={`${id}-tab-${key}`} hidden={i !== index} tabIndex={0} className={`mg-slide${key === "early-lane" || key === "mid-lane" ? " is-visual-slide" : ""}`}>
      <header className="mg-slide-heading"><h2>{label}</h2><small>{i + 1} / {count}</small></header>
      {key === "early-lane" && <TradeWindows champion={champion} enemy={enemy} />}
      {key === "mid-lane" && <PowerSpikeTimeline champion={champion} enemy={enemy} lane={lane} tier={tier} matchup={matchup} />}
      <div className="mg-slide-reading" tabIndex={key === "early-lane" || key === "mid-lane" ? 0 : undefined} aria-label={`${label} tips`}>
      {loading ? <Skeleton label="Loading guide" rows={3} height={50} /> : guide
        ? key === "overview" ? <MatchupGuideSummary guide={guide} abilities={abilities} /> : <GuideSectionContent guide={guide} section={key} abilities={abilities} />
        : <p className="mg-no-written-guide">No written guide yet.</p>}
      </div>
    </div>)}</div>
    <div className="mg-navigation">
      <button aria-label="Previous guide section" onClick={() => setIndex(guideStep(index, "ArrowLeft")!)}><ChevronLeft size={17} /><span>Previous</span></button>
      <div className="mg-progress"><div className="mg-dots" aria-label="Guide progress">{GUIDE_SECTIONS.map(([key, label], i) => <button key={key} aria-label={`Go to ${label}`} aria-current={i === index ? "step" : undefined} onClick={() => setIndex(i)} />)}</div>
        <small aria-live="polite">{index + 1} of {count} · {GUIDE_SECTIONS[index][1]}</small></div>
      <button aria-label="Next guide section" onClick={() => setIndex(guideStep(index, "ArrowRight")!)}><span>Next</span><ChevronRight size={17} /></button>
    </div>
    </div>
  </section>;
}
