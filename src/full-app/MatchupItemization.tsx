import type { ChampionBuild, MatchupView } from "../runes/types";
import { GuideTips, type GuideAbilities } from "./MatchupGuideContent";
import { itemChanges, type AuthoredGuide } from "./matchupGuide";
import { fmtCount } from "./ui/format";

export function MatchupItemization({ detail, usual, guide, abilities }: {
  detail: MatchupView | null; usual: ChampionBuild | null; guide: AuthoredGuide | null; abilities: GuideAbilities;
}) {
  const changes = usual && detail?.build && !detail.fallback ? itemChanges(usual.items.items, detail.build.items) : [];
  return <section className="ui-card mg-itemization" aria-label="Itemization">
    <h3>Itemization</h3>
    <div className="mg-build-diff"><strong>Compared with Most played</strong>
      <p>{detail?.fallback ? "Too few games for a matchup build. Showing the general build."
        : !usual || !detail?.build ? "Build comparison is not available yet."
        : changes.length ? changes.join(" · ") : "No item order changes in the available matchup data."}</p>
      {detail?.stats && <small>{fmtCount(detail.stats.games)} matchup games · Data patch {detail.stats.patch}</small>}
    </div>
    {guide && <GuideTips tips={guide.itemization} sources={guide.sources} abilities={abilities} />}
  </section>;
}
