import { GuideTips, type GuideAbilities } from "./MatchupGuideContent";
import type { MatchupGuide } from "./matchupGuide";
import "./matchup-guide.css";

export function MatchupGuideSummary({ guide, abilities }: { guide: MatchupGuide; abilities?: GuideAbilities }) {
  return <section className="mg-summary" aria-label="Matchup summary">
    <div className="mg-summary-outlook"><span className="mg-difficulty">{guide.difficulty}</span>
      <h3>Who has the edge?</h3><p>{guide.favoredWhen}</p>
      <small>Play around the window, not the win rate.</small>
    </div>
    <div><h3>Three things to remember</h3><GuideTips tips={guide.keyTips} sources={guide.sources} abilities={abilities} /></div>
  </section>;
}
