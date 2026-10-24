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
  if (!view) return <section className="lgc" aria-label="Live game"><h2><i className="lgc-dot" />Live game</h2><p className="lgc-wait">Loading game…</p></section>;
  return <section className="lgc" aria-label="Live game">
    <h2><i className="lgc-dot" />Live game<small>{view.queue}</small></h2>
    <div className="lgc-teams">
      {(["blue", "red"] as const).map(side => <section key={side} className={`lgc-team is-${side}`} aria-label={`${side === "blue" ? "Blue" : "Red"} team`}>
        <h3>{side === "blue" ? "Blue team" : "Red team"}</h3>
        <ol>
          {view.players.filter(p => p.side === side).map(p => {
            const [name] = splitRiotId(p.riotId || p.championName);
            return <li key={p.key} className={p.isSelf ? "is-self" : undefined} title={`${p.riotId} · ${rankLabel(p.solo)}`}>
              <ChampionIcon id={p.championId} name={p.championName} mode="desktop" />
              <span className="lgc-name">{name}{p.isSelf && <em>You</em>}</span>
              {p.status === "pending" ? <small aria-label="Loading rank">…</small> : <span className={`lgc-rank${p.solo ? "" : " is-unranked"}`} aria-label={rankLabel(p.solo)}>{p.solo && <RankCrest tier={crestSlug(p.solo.tier)} size={20} mode="desktop" />}<small>{shortRank(p.solo)}</small></span>}
            </li>;
          })}
        </ol>
      </section>)}
    </div>
  </section>;
}

/** "E3", "GM", "U" (unranked): op.gg style, to fit the flyout. */
function shortRank(rank: Rank | null): string {
  if (!rank) return "U";
  if (rank.tier === "GRANDMASTER") return "GM";
  return rank.tier.charAt(0) + ((({ I: "1", II: "2", III: "3", IV: "4" }) as Record<string, string>)[rank.division] ?? "");
}
