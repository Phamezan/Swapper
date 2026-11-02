import { ChampionIcon } from "../runes/ChampionIcon";
import type { ChampionOption } from "../runes/types";
import type { MatchupVod } from "./matchupGuide";
import { championsInMoment } from "./matchupVodModel";
import { MomentIcon } from "./MomentIcon";

/** Portraits follow the written label; we don't infer killer/victim from prose. */
export function VodMomentArt({ moment, champions, index }: {
  moment: MatchupVod['moments'][number]; champions: readonly ChampionOption[]; index: number;
}) {
  const named = championsInMoment(moment.label, champions).slice(0, 2);
  return <span className="mv-event-art" data-kind={moment.kind} aria-hidden>
    {named.length ? <span className="mv-portraits">{named.map(champion => <ChampionIcon key={champion.id} id={champion.id} name={champion.name} mode="desktop" />)}</span>
      : <span className="mv-event-number">{String(index + 1).padStart(2, '0')}</span>}
    <MomentIcon kind={moment.kind} size={16} />
  </span>;
}
