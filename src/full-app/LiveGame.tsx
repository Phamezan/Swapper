import { useState, type CSSProperties } from "react";
import { ChampionIcon } from "../runes/ChampionIcon";
import { RankCrest } from "../runes/RankCrest";
import { RoleIcon } from "../runes/RoleIcon";
import { RuneIcon } from "../runes/RuneIcon";
import { SpellIcon } from "../runes/SpellIcon";
import { BrandIcon } from "../components/BrandIcon";
import { useLiveGame } from "./LiveGameData";
import { clock, crestSlug, hasRecord, kda, pct, rankLabel, roleOf, splitRiotId, titleCase, type Player, type TeamSummary } from "./LiveGameModel";
import { LiveGamePlayer } from "./LiveGamePlayer";
import { Slot } from "./ui/Slot";
import "./live-game.css";

const roleNames: Record<string, string> = { top: "Top", jungle: "Jungle", mid: "Mid", adc: "Bot", support: "Support" };

/** Every player in the running game: rank, recent form, champion stats and,
 *  while this page is visible, live K/D/A, CS and items. */
export default function LiveGame({ championNames }: { championNames: Map<number, string> }) {
  const { view, error, loaded } = useLiveGame(true);
  const [selected, setSelected] = useState<string | null>(null);
  const player = view?.players.find(p => p.key === selected) ?? null;
  const selfSide = view?.players.find(p => p.isSelf)?.side;
  const fetched = view?.players.filter(p => p.status !== "pending").length ?? 0;
  if (!view) {
    return <section className="empty lg-empty">
      <BrandIcon size={52} />
      <h2>{loaded ? "Loading game…" : "Reading game…"}</h2>
      <p>{error || "Players appear once the game loads."}</p>
    </section>;
  }
  return <div className="lg">
    <div className="lg-bar">
      <span className="lg-queue">{view.queue || "Custom game"}</span>
      {view.gameTime !== null && <span className="lg-clock" title="Game time">{clock(view.gameTime)}</span>}
      <small>{fetched < view.players.length ? `Loading players ${fetched}/${view.players.length}…` : "Rank and form: latest 20 Rift games"}</small>
    </div>
    {(["blue", "red"] as const).map(side => <Team key={side} side={side} summary={view[side]}
      players={view.players.filter(p => p.side === side)} live={view.gameTime !== null} onOpen={setSelected} />)}
    {player && <LiveGamePlayer player={player} championNames={championNames} enemy={player.side !== selfSide} onClose={() => setSelected(null)} />}
  </div>;
}

function Team({ side, summary, players, live, onOpen }: {
  side: "blue" | "red"; summary: TeamSummary; players: Player[]; live: boolean; onOpen: (key: string) => void;
}) {
  const kills = players.reduce((n, p) => n + (p.live?.kills ?? 0), 0);
  return <section className={`panel lg-team is-${side}`} aria-label={`${titleCase(side)} team`}>
    <header className="lg-team-head">
      <h2>{side === "blue" ? "Blue team" : "Red team"}</h2>
      {live && <b className="lg-team-kills" title="Team kills">{kills}</b>}
      <span className="lg-avg" title="Average Solo/Duo rank of the ranked players">
        <RankCrest tier={crestSlug(summary.averageTier)} size={18} mode="desktop" />
        {summary.averageTier ? `${titleCase(summary.averageTier)} ${summary.averageDivision}`.trim() : "Unranked"}
        <small>avg · {summary.ranked}/{players.length} ranked</small>
      </span>
      <DamageBar physical={summary.physical} />
    </header>
    <div className="lg-cols" aria-hidden>
      <span>Player</span><span>Solo/Duo</span><span className="lg-flex">Flex</span><span>Latest 20</span><span>On champion</span><span className="lg-role">Role</span><span>{live ? "Live" : ""}</span>
    </div>
    <ol>{players.map(p => <Row key={p.key} player={p} onOpen={() => onOpen(p.key)} />)}</ol>
  </section>;
}

function DamageBar({ physical }: { physical: number | null }) {
  if (physical === null) return <span className="lg-dmg is-empty" title="Damage profile appears once every champion is known" />;
  return <span className="lg-dmg" title={`About ${physical}% physical and ${100 - physical}% magic damage dealers (champion damage types)`}>
    <small>AD {physical}%</small>
    <i style={{ "--ad": `${physical}%` } as CSSProperties} aria-hidden />
    <small>AP {100 - physical}%</small>
  </span>;
}

function Row({ player: p, onOpen }: { player: Player; onOpen: () => void }) {
  const [name, tag] = splitRiotId(p.riotId || p.championName);
  const h = p.history;
  const loading = p.status === "pending";
  const assigned = roleOf(p.position);
  return <li>
    <button className={`lg-row${p.isSelf ? " is-self" : ""}${p.live?.dead ? " is-dead" : ""}`} onClick={onOpen} title={`Open ${name}'s profile`}>
      <span className="lg-champ">
        <span className="lg-portrait">
          <ChampionIcon id={p.championId} name={p.championName} mode="desktop" />
          {p.live && <b title="Level">{p.live.level}</b>}
        </span>
        <span className="lg-stack">{p.spells.map((id, i) => <SpellIcon key={i} id={id} mode="desktop" />)}</span>
        <span className="lg-stack lg-runes">
          {p.keystone > 0 ? <RuneIcon id={p.keystone} mode="desktop" /> : <span className="rune-icon-fallback" />}
          {p.secondaryStyle > 0 ? <RuneIcon id={p.secondaryStyle} mode="desktop" className="lg-tree" /> : <span className="rune-icon-fallback" />}
        </span>
      </span>
      <span className="lg-name">
        <strong><span className="riot-name">{name}</span><span className="riot-tag">{tag}</span>{p.isSelf && <em className="lg-you">You</em>}</strong>
        <span className="lg-badges">
          {p.isBot ? <small>Bot</small> : loading ? <small className="lg-skel">Loading…</small>
            : p.badges.length ? p.badges.map(b => <span key={b.kind} className={`lg-badge is-${b.kind}`} title={b.detail}>{b.label}</span>)
            : <small>{p.championName}{p.summonerLevel ? ` · Lv ${p.summonerLevel}` : ""}</small>}
        </span>
      </span>
      <span className="lg-rank">
        {loading ? <i className="lg-skel-bar" /> : <>
          <RankCrest tier={crestSlug(p.solo?.tier)} size={26} mode="desktop" />
          <span><strong>{rankLabel(p.solo)}</strong>
            <small>{p.solo ? (hasRecord(p.solo) ? `${p.solo.lp} LP · ${p.solo.wins}W ${p.solo.losses}L · ${pct(p.solo.wins, p.solo.wins + p.solo.losses)}%` : `${p.solo.lp} LP`) : p.status === "skipped" ? "–" : "No Solo/Duo rank"}</small></span>
        </>}
      </span>
      <span className="lg-flex">{loading ? <i className="lg-skel-bar" /> : <><strong>{p.flex ? rankLabel(p.flex) : "–"}</strong>{p.flex && <small>{p.flex.lp} LP</small>}</>}</span>
      <span className="lg-form">{h && h.form.games ? <>
        <strong className={pct(h.form.wins, h.form.games) >= 50 ? "up" : "down"}>{pct(h.form.wins, h.form.games)}%</strong>
        <small>{h.form.games} games · {kda(h.form).toFixed(1)} KDA</small>
      </> : <small>{loading ? "" : "No Rift games"}</small>}</span>
      <span className="lg-oncha">{h && h.onChampion.games ? <>
        <strong className={pct(h.onChampion.wins, h.onChampion.games) >= 50 ? "up" : "down"}>{pct(h.onChampion.wins, h.onChampion.games)}%</strong>
        <small>{h.onChampion.games} {h.onChampion.games === 1 ? "game" : "games"} · {kda(h.onChampion).toFixed(1)}</small>
      </> : <small>{h ? "No recent games" : ""}</small>}</span>
      <span className="lg-role" title={h?.mainRole ? `Usually ${roleNames[h.mainRole[0]]} (${h.mainRole[1]} of ${h.form.games}, estimated)${assigned ? `; playing ${roleNames[assigned]}` : ""}` : undefined}>
        {h?.mainRole ? <><RoleIcon role={h.mainRole[0]} size={18} mode="desktop" /><small>{h.mainRole[1]}/{h.form.games}</small></> : assigned ? <RoleIcon role={assigned} size={18} mode="desktop" /> : null}
      </span>
      <span className="lg-live">{p.live ? <>
        <span className="lg-kda"><strong>{p.live.kills} <i>/</i> <em>{p.live.deaths}</em> <i>/</i> {p.live.assists}</strong><small>{p.live.cs} CS</small></span>
        <span className="lg-items">{Array.from({ length: 7 }, (_, i) => <Slot key={i} kind="item" id={p.live!.items[i]} className={i === 6 ? "is-trinket" : undefined} />)}</span>
      </> : null}</span>
    </button>
  </li>;
}
