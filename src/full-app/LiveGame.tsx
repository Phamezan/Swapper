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
      players={view.players.filter(p => p.side === side)} live={view.gameTime !== null || view.players.some(p => p.live !== null)} onOpen={setSelected} />)}
    {player && <LiveGamePlayer player={player} championNames={championNames} enemy={player.side !== selfSide} onClose={() => setSelected(null)} />}
  </div>;
}

function Team({ side, summary, players, live, onOpen }: {
  side: "blue" | "red"; summary: TeamSummary; players: Player[]; live: boolean; onOpen: (key: string) => void;
}) {
  const kills = players.reduce((n, p) => n + (p.live?.kills ?? 0), 0);
  return <section className={`panel lg-team is-${side}${live ? "" : " is-pregame"}`} aria-label={`${titleCase(side)} team`}>
    <header className="lg-team-head">
      <div className="lg-team-identity">
        <h2>{side === "blue" ? "Blue team" : "Red team"}</h2>
        <small>{players.some(p => p.isSelf) ? "Your team · " : ""}{players.length} {players.length === 1 ? "player" : "players"}</small>
      </div>
      {live && <span className="lg-team-score"><b className="lg-team-kills">{kills}</b><small>Kills</small></span>}
      <span className="lg-avg" title="Average Solo/Duo rank of the ranked players">
        {summary.averageTier && <RankCrest tier={crestSlug(summary.averageTier)} size={32} mode="desktop" />}
        <span><small>Average Solo/Duo · {summary.ranked}/{players.length} ranked</small>
          <strong>{summary.averageTier ? `${titleCase(summary.averageTier)} ${summary.averageDivision}`.trim() : "Unranked"}</strong></span>
      </span>
      <DamageBar physical={summary.physical} />
    </header>
    <div className="lg-cols" aria-hidden>
      <span>Player &amp; loadout</span><span>Ranked Solo/Duo <small>+ Flex</small></span><span>Recent form</span>{live && <span>Live stats &amp; items</span>}
    </div>
    <ol>{players.map(p => <Row key={p.key} player={p} live={live} onOpen={() => onOpen(p.key)} />)}</ol>
  </section>;
}

function DamageBar({ physical }: { physical: number | null }) {
  if (physical === null) return <span className="lg-dmg is-empty"><small>Damage profile</small><span>Waiting for champions</span></span>;
  return <span className="lg-dmg" title={`About ${physical}% physical and ${100 - physical}% magic damage dealers (champion damage types)`}>
    <span className="lg-dmg-labels"><small>AD <b>{physical}%</b></small><small>AP <b>{100 - physical}%</b></small></span>
    <i style={{ "--ad": `${physical}%` } as CSSProperties} aria-hidden />
  </span>;
}

function Row({ player: p, live, onOpen }: { player: Player; live: boolean; onOpen: () => void }) {
  const [name, tag] = splitRiotId(p.riotId || p.championName);
  const h = p.history;
  const loading = p.status === "pending";
  const assigned = roleOf(p.position);
  return <li>
    <button className={`lg-row${p.isSelf ? " is-self" : ""}${p.live?.dead ? " is-dead" : ""}`} onClick={onOpen} title={`Open ${name}'s profile`} aria-haspopup="dialog">
      <span className="lg-identity">
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
          <span className="lg-player-meta"><small>{p.championName}{p.summonerLevel ? ` · Lv ${p.summonerLevel}` : ""}</small>
            <span className="lg-role" title={h?.mainRole ? `Usually ${roleNames[h.mainRole[0]]} (${h.mainRole[1]} of ${h.form.games}, estimated)${assigned ? `; playing ${roleNames[assigned]}` : ""}` : assigned ? `Playing ${roleNames[assigned]}` : undefined}>
              {h?.mainRole ? <><RoleIcon role={h.mainRole[0]} size={14} mode="desktop" /><small>{h.mainRole[1]}/{h.form.games}</small></> : assigned ? <RoleIcon role={assigned} size={14} mode="desktop" /> : null}
            </span>
          </span>
          {(p.isBot || loading || p.badges.length > 0) && <span className="lg-badges">
            {p.isBot ? <span className="lg-badge">Bot</span> : loading ? <small className="lg-skel">Loading…</small>
              : p.badges.map(b => <span key={b.kind} className={`lg-badge is-${b.kind}`} title={b.detail}>{b.label}</span>)}
          </span>}
        </span>
      </span>
      <span className={`lg-rank${!p.solo && !loading ? " is-unranked" : ""}`}>
        {loading ? <i className="lg-skel-bar" /> : <>
          {p.solo && <RankCrest tier={crestSlug(p.solo.tier)} size={44} mode="desktop" />}
          <span><strong>{rankLabel(p.solo)}</strong>
            <small className="lg-rank-record">{p.solo ? <><b>{p.solo.lp} LP</b>{hasRecord(p.solo) && <> · {p.solo.wins}W {p.solo.losses}L · {pct(p.solo.wins, p.solo.wins + p.solo.losses)}%</>}</> : p.status === "skipped" ? "–" : "No Solo/Duo rank"}</small>
            <small className="lg-flex">Flex <span>{p.flex ? `${rankLabel(p.flex)} · ${p.flex.lp} LP` : "–"}</span></small>
          </span>
        </>}
      </span>
      <span className="lg-performance">
        <span className="lg-form"><small className="lg-stat-label">Latest 20</small>{h && h.form.games ? <>
          <strong className={pct(h.form.wins, h.form.games) >= 50 ? "up" : "down"}>{pct(h.form.wins, h.form.games)}<span>%</span></strong>
          <small>{h.form.games} games · {kda(h.form).toFixed(1)} KDA</small>
        </> : <small className="lg-no-data">{loading ? "Loading…" : "No Rift games"}</small>}</span>
        <span className="lg-oncha"><small className="lg-stat-label">On champion</small>{h && h.onChampion.games ? <>
          <strong className={pct(h.onChampion.wins, h.onChampion.games) >= 50 ? "up" : "down"}>{pct(h.onChampion.wins, h.onChampion.games)}<span>%</span></strong>
          <small>{h.onChampion.games} {h.onChampion.games === 1 ? "game" : "games"} · {kda(h.onChampion).toFixed(1)} KDA</small>
        </> : <small className="lg-no-data">{h ? "No recent games" : loading ? "Loading…" : "Unavailable"}</small>}</span>
      </span>
      {live && <span className="lg-live">{p.live ? <>
        <span className="lg-kda"><strong aria-label={`${p.live.kills} kills, ${p.live.deaths} deaths, ${p.live.assists} assists`}>{p.live.kills} <i>/</i> <em>{p.live.deaths}</em> <i>/</i> {p.live.assists}</strong><small>{p.live.cs} CS{p.live.dead && <em className="lg-dead"> · Dead</em>}</small></span>
        {p.live.items.some(id => id > 0) ? <span className="lg-items">{Array.from({ length: 7 }, (_, i) => <Slot key={i} kind="item" id={p.live!.items[i]} className={i === 6 ? "is-trinket" : undefined} />)}</span> : <small className="lg-no-items">No items yet</small>}
      </> : <small className="lg-no-data">Waiting for live stats</small>}</span>}
    </button>
  </li>;
}
