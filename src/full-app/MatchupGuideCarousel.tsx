import { useEffect, useId, useRef, useState } from "react";
import { ChevronLeft, ChevronRight } from "lucide-react";
import type { ChampionOption, MatchupView } from "../runes/types";
import { Skeleton } from "./ui/States";
import { GuideSectionContent } from "./MatchupGuideContent";
import { MatchupGuideSummary } from "./MatchupGuideSummary";
import { GUIDE_SECTIONS, guideStep, type GuideSectionId } from "./matchupGuide";
import type { useMatchupGuide } from "./useMatchupGuide";
import { DataCardOverview, DataCardEnemyAbilities } from "./MatchupDataCardView";
import { enemySpells } from "./matchupDataCard";
import { TradeWindows } from "./TradeWindows";
import { PowerSpikeTimeline } from "./PowerSpikeTimeline";
import "./matchup-guide.css";

export function MatchupGuideCarousel({ guide, content, loading, abilities, champion, enemy, lane, tier, matchup, initialSection }: Pick<ReturnType<typeof useMatchupGuide>, "guide" | "loading" | "abilities" | "content"> & {
  champion: ChampionOption; enemy: ChampionOption; lane: string; tier: string; matchup: MatchupView | null;
  initialSection?: GuideSectionId;
}) {
  const data = content?.kind === "data-based" ? content : null;
  const sections: readonly (readonly [GuideSectionId, string])[] = data ? [["overview", "Matchup data"], ...(enemySpells(data).length ? [["early-lane", "Enemy abilities"] as const] : [])] : GUIDE_SECTIONS;
  const count = sections.length;
  const sourceKey = `${champion.id}-${enemy.id}-${lane}-${tier}-${data ? "data" : guide?.enemy === undefined ? "general" : "exact"}-${count}`;
  const [index, setIndex] = useState(() => Math.max(0, sections.findIndex(([section]) => section === initialSection)));
  const source = useRef(sourceKey);
  useEffect(() => { if (source.current !== sourceKey) { source.current = sourceKey; setIndex(0); } }, [sourceKey]);
  const tabs = useRef<HTMLDivElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  const id = useId();
  useEffect(() => { if (initialSection) tabs.current?.querySelector<HTMLButtonElement>('[aria-selected="true"]')?.focus({ preventScroll: true }); }, [initialSection]);
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.defaultPrevented || event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
      const target = event.target instanceof HTMLElement ? event.target : null;
      if (target?.closest('input, textarea, select, [contenteditable="true"], [role="slider"], [role="combobox"], [data-radix-popper-content-wrapper]')) return;
      const otherTabs = target?.closest('[role="tablist"]');
      if (otherTabs && otherTabs !== tabs.current) return;
      const next = guideStep(index, event.key, count);
      if (next === null) return;
      event.preventDefault(); setIndex(next);
      if (tabs.current?.contains(target) || panel.current?.contains(target)) tabs.current?.querySelectorAll<HTMLButtonElement>('[role="tab"]')[next]?.focus();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [index, count]);
  useEffect(() => { tabs.current?.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: "nearest", inline: "nearest" }); }, [index]);
  return <section className="mg-deck" aria-label="Matchup guide">
    {!data && guide && guide.enemy === undefined && <p className="mg-no-written-guide">General {champion.name} guide · no {enemy.name} matchup guide yet</p>}
    <div ref={tabs} className="mg-tabs" role="tablist" aria-label="Guide sections">{sections.map(([key, label], i) =>
      <button key={key} id={`${id}-tab-${key}`} role="tab" aria-selected={i === index} aria-controls={`${id}-panel-${key}`} tabIndex={i === index ? 0 : -1} onClick={() => setIndex(i)}>{label}</button>)}
    </div>
    <div className="mg-stage" ref={panel}>{sections.map(([key, label], i) => <div key={key} id={`${id}-panel-${key}`} role="tabpanel" aria-labelledby={`${id}-tab-${key}`} hidden={i !== index} tabIndex={0} className={`mg-slide${!data && (key === "early-lane" || key === "mid-lane") ? " is-visual-slide" : ""}`}>
      <header className="mg-slide-heading"><h2>{label}</h2><small>{i + 1} / {count}</small></header>
      {!data && key === "early-lane" && <TradeWindows champion={champion} enemy={enemy} />}
      {!data && key === "mid-lane" && <PowerSpikeTimeline champion={champion} enemy={enemy} lane={lane} tier={tier} matchup={matchup} />}
      <div className="mg-slide-reading" tabIndex={!data && (key === "early-lane" || key === "mid-lane") ? 0 : undefined} aria-label={`${label} tips`}>
      {data ? key === "overview" ? <DataCardOverview card={data} champion={champion} enemy={enemy} /> : <DataCardEnemyAbilities card={data} enemy={enemy} /> : loading ? <Skeleton label="Loading guide" rows={3} height={50} /> : guide
        ? key === "overview" ? <MatchupGuideSummary guide={guide} abilities={abilities} /> : <GuideSectionContent guide={guide} section={key} abilities={abilities} />
        : <p className="mg-no-written-guide">No written guide yet.</p>}
      </div>
    </div>)}</div>
    {count > 1 && <div className="mg-navigation">
      <button aria-label="Previous guide section" onClick={() => setIndex(guideStep(index, "ArrowLeft", count)!)}><ChevronLeft size={17} /><span>Previous</span></button>
      <div className="mg-progress"><div className="mg-dots" aria-label="Guide progress">{sections.map(([key, label], i) => <button key={key} aria-label={`Go to ${label}`} aria-current={i === index ? "step" : undefined} onClick={() => setIndex(i)} />)}</div>
        <small aria-live="polite">{index + 1} of {count} · {sections[Math.min(index, count - 1)][1]}</small></div>
      <button aria-label="Next guide section" onClick={() => setIndex(guideStep(index, "ArrowRight", count)!)}><span>Next</span><ChevronRight size={17} /></button>
    </div>}
  </section>;
}
