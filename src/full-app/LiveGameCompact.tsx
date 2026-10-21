import { ChampionIcon } from "../runes/ChampionIcon";
import { RankCrest } from "../runes/RankCrest";
import { useInGame, useLiveGame } from "./LiveGameData";
import { crestSlug, rankLabel, splitRiotId, type Rank } from "./LiveGameModel";
import "./live-game-compact.css";

/** The flyout's Live Game card: both teams with champion and Solo/Duo rank.
 *  Renders nothing outside a game; no Live Client reads (the Fullsized App is
 *  the full surface). */
export function LiveGameCompact() {
  return useInGame() ? <Card /> : null;
}

function Card() {
  const { view } = useLiveGame(false);
  if (!view) return <section className="lgc" aria-label="Live game"><h2><i className="lgc-dot" />Live game</h2><p className="lgc-wait">Waiting for the game to load…</p></section>;
  return <section className="lgc" aria-label="Live game">
    <h2><i className="lgc-dot" />Live game<small>{view.queue}</small></h2>
    <div className="lgc-teams">
      {(["blue", "red"] as const).map(side => <ol key={side} className={`is-${side}`}>
        {view.players.filter(p => p.side === side).map(p => {
          const [name] = splitRiotId(p.riotId || p.championName);
          return <li key={p.key} className={p.isSelf ? "is-self" : undefined} title={`${p.riotId} · ${rankLabel(p.solo)}`}>
            <ChampionIcon id={p.championId} name={p.championName} mode="desktop" />
            <span className="lgc-name">{name}</span>
            {p.status === "pending" ? <small>…</small> : <span className="lgc-rank"><RankCrest tier={crestSlug(p.solo?.tier)} size={14} mode="desktop" /><small>{shortRank(p.solo)}</small></span>}
          </li>;
        })}
      </ol>)}
    </div>
  </section>;
}

/** "E3", "GM", "U" (unranked): op.gg style, to fit the flyout. */
function shortRank(rank: Rank | null): string {
  if (!rank) return "U";
  if (rank.tier === "GRANDMASTER") return "GM";
  return rank.tier.charAt(0) + ((({ I: "1", II: "2", III: "3", IV: "4" }) as Record<string, string>)[rank.division] ?? "");
}
