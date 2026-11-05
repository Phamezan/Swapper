import type { ChampionOption } from "../runes/types";
import type { RunesPanelProps } from "../runes/RunesPanelProps";
import { DataCardLabel, DataCardOverview } from "./MatchupDataCardView";
import { MatchupGuideSummary } from "./MatchupGuideSummary";
import { useMatchupGuide } from "./useMatchupGuide";

export function ChampionSelectMatchup({ champion, enemyId, enemyName, lane, tier = "emerald_plus", allowData = true, onOpenMatchupGuide }: {
  champion: ChampionOption; enemyId: number; enemyName: string; lane: string; tier?: string; allowData?: boolean;
  onOpenMatchupGuide?: RunesPanelProps["onOpenMatchupGuide"];
}) {
  const { guide, content, record, abilities } = useMatchupGuide(champion.id, enemyId, lane, tier, allowData);
  const data = content?.kind === "data-based" ? content : null;
  const hasRecord = record && record.wins + record.losses > 0;
  if (!guide && !data && !hasRecord) return null;
  const recordLine = hasRecord && <span className="cs-guide-record" aria-label="Your matchup record">
    <small>Your record</small><strong>{record.wins}W – {record.losses}L</strong>
  </span>;
  if (!guide && !data) return <div className="cs-guide-record-only">{recordLine}</div>;
  return <details className="cs-matchup-guide">
    <summary><span className="cs-guide-heading">{data ? <>{champion.name} vs {enemyName} <DataCardLabel /></> : guide?.enemy === undefined ? `General ${champion.name} guide · no ${enemyName} matchup guide yet` : <>Matchup guide <span className="cs-guide-difficulty">{guide?.difficulty}</span></>}{recordLine}</span>
      {!data && guide?.enemy !== undefined && <span className="cs-guide-outlook">{guide?.favoredWhen}</span>}</summary>
    <div className="cs-guide-reading">{data ? <DataCardOverview card={data} champion={champion} enemy={{ id: enemyId, name: enemyName }} /> : guide && <MatchupGuideSummary guide={guide} abilities={abilities} />}</div>
    <div className="cs-guide-footer"><button type="button" disabled={!onOpenMatchupGuide}
      onClick={() => onOpenMatchupGuide?.(champion, enemyId, lane)}>Open full guide</button></div>
  </details>;
}
