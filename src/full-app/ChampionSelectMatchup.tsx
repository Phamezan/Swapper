import type { ChampionOption } from "../runes/types";
import type { RunesPanelProps } from "../runes/RunesPanelProps";
import { MatchupGuideSummary } from "./MatchupGuideSummary";
import { useMatchupGuide } from "./useMatchupGuide";

export function ChampionSelectMatchup({ champion, enemyId, lane, onOpenMatchupGuide }: {
  champion: ChampionOption; enemyId: number; lane: string;
  onOpenMatchupGuide?: RunesPanelProps["onOpenMatchupGuide"];
}) {
  const { guide, record, abilities } = useMatchupGuide(champion.id, enemyId, lane);
  const hasRecord = record && record.wins + record.losses > 0;
  if (!guide && !hasRecord) return null;
  const recordLine = hasRecord && <span className="cs-guide-record" aria-label="Your matchup record">
    <small>Your record</small><strong>{record.wins}W – {record.losses}L</strong>
  </span>;
  if (!guide) return <div className="cs-guide-record-only">{recordLine}</div>;
  return <details className="cs-matchup-guide">
    <summary><span className="cs-guide-heading">Matchup guide <span className="cs-guide-difficulty">{guide.difficulty}</span>{recordLine}</span>
      <span className="cs-guide-outlook">{guide.favoredWhen}</span></summary>
    <div className="cs-guide-reading"><MatchupGuideSummary guide={guide} abilities={abilities} /></div>
    <div className="cs-guide-footer"><button type="button" disabled={!onOpenMatchupGuide}
      onClick={() => onOpenMatchupGuide?.(champion, enemyId, lane)}>Open full guide</button></div>
  </details>;
}
