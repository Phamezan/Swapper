import { useEffect, useLayoutEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { ObjectiveIcon } from "./ObjectiveIcon";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ChampionIcon } from "../runes/ChampionIcon";
import { SpellIcon } from "../runes/SpellIcon";
import { RankCrest } from "../runes/RankCrest";
import { fmtAgo } from "../runes/types";
import { crestSlug, queueLabels, splitRiotId, titleCase } from "./Dashboard";
import { useChampionArt } from "./art";
import { barWidth, matchFacts, percent, placementLabel, rankLabel, scoreTone, teamAverageDamage, teamTotals, type MatchPlayer as Player, type MatchView } from "./MatchDetailModel";
import type { Game, Snapshot } from "./types";
import { Comparison } from "./match/Comparison";
import { Stats } from "./match/Stats";
import { Timeline } from "./match/Timeline";
import { Facts } from "./match/Facts";
import { Slot } from "./ui/Slot";
import { Notice, Skeleton } from "./ui/States";
import { fmtCompact, fmtCount } from "./ui/format";
import "./MatchDetail.css";

const stat = (value: number | null | undefined, digits = 0) => value == null ? "–" : digits ? value.toFixed(digits) : fmtCount(value);
const compact = (value: number | null | undefined) => value == null ? "–" : fmtCompact(value);
const kda = (p: Player) => p.kills == null || p.deaths == null || p.assists == null ? null : (p.kills + p.assists) / Math.max(1, p.deaths);
const ITEM_SLOTS = 7;
const participation = (p: Player, total: number | null) => percent(p.kills != null && p.assists != null ? p.kills + p.assists : null, total);

export default function MatchDetail({ game, names, riotId }: { game: Game; names: Snapshot["championNames"]; riotId?: string }) {
  const [detail, setDetail] = useState<MatchView | null>(null);
  const [error, setError] = useState("");
  const [attempt, setAttempt] = useState(0);
  const [rankPending, setRankPending] = useState(true);
  const [tab, setTab] = useState("Overview");
  const tabBar = useRef<HTMLDivElement>(null);
  const scrollRequest = useRef<{ scroller: HTMLElement; top: number } | null>(null);
  const selectTab = (label: string) => {
    if (label === tab) return;
    const scroller = tabBar.current?.closest<HTMLElement>(".dashboard-main");
    if (scroller && tabBar.current) {
      const bounds = tabBar.current.getBoundingClientRect(), viewport = scroller.getBoundingClientRect();
      if (bounds.top < viewport.top || bounds.bottom > viewport.bottom) {
        scrollRequest.current = { scroller, top: scroller.scrollTop + bounds.top - viewport.top };
      }
    }
    setTab(label);
  };
  useLayoutEffect(() => {
    const request = scrollRequest.current;
    scrollRequest.current = null;
    if (request) request.scroller.scrollTo({ top: request.top, behavior: window.matchMedia("(prefers-reduced-motion: reduce)").matches ? "instant" : "smooth" });
  }, [tab]);
  const splash = useChampionArt(game.championId, "splash");
  useEffect(() => {
    let live = true; setDetail(null); setError(""); setRankPending(true); setTab("Overview");
    scrollRequest.current = null;
    const expected = riotId ?? game.players.find(p => p.me)?.riotId;
    const stop = listen<MatchView>("match_detail", ({ payload }) => {
      if (live && payload.game.id === game.id && (!expected || payload.players.some(p => p.me && p.riotId.toLowerCase() === expected.toLowerCase()))) setDetail(payload);
    });
    void stop.then(() => live ? invoke<MatchView>("full_app_match_detail", { gameId: game.id, riotId }) : undefined)
      .then(value => { if (live && value) setDetail(value); }).catch(reason => { if (live) setError(String(reason)); })
      .finally(() => { if (live) setRankPending(false); });
    return () => { live = false; void stop.then(unlisten => unlisten()); };
  }, [game.id, riotId, attempt]);
  const match = detail?.game ?? game;
  const own = detail?.players.find(p => p.me);
  const teamRows = useMemo(() => detail?.teams.map(team => {
    const players = detail.players.filter(p => p.teamId === team.teamId);
    return { team, players, total: teamTotals(players), best: bestOf(players), average: teamAverageDamage(players) };
  }) ?? [], [detail]);
  const ownTeam = teamRows.find(row => row.team.teamId === own?.teamId);
  const ownLabel = own ? placementLabel(own.placement, ownTeam?.team.win ?? null, own === ownTeam?.best) : "–";
  const name = (id: number) => names[id] ?? `Champion ${id}`;
  const duration = `${Math.floor(match.duration / 60)}:${String(match.duration % 60).padStart(2, "0")}`;
  const minutes = match.duration / 60;
  const facts = useMemo(() => detail ? matchFacts(detail.players, detail.teams) : [], [detail]);
  const tabs = ["Overview", "Scoreboard", "Loadout", "Stats", ...(detail?.timeline ? ["Graphs"] : [])];
  const maxDamage = useMemo(() => Math.max(1, ...(detail?.players.map(p => p.damage ?? 0) ?? [])), [detail]);
  const hero = <section className={`md-header ${match.win ? "is-win" : "is-loss"}`} aria-label="Your match performance">
      {splash && <img className="md-art" src={splash} alt="" draggable={false} loading="lazy" />}
      <div className="md-header-main">
        <div className="md-story"><div className="md-match-meta"><span>{queueLabels[match.queue] ?? match.queue}</span><span>{duration}</span><span>{fmtAgo(match.createdAt)}</span></div>
          <div className="md-outcome"><h2>{match.win ? "Victory" : "Defeat"}</h2>{ownLabel !== "–" && <Placement label={ownLabel} long />}</div>
          <div className="md-hero-champion"><ChampionIcon id={match.championId} name={name(match.championId)} mode="desktop" /><strong>{name(match.championId)}</strong><span>Level {own?.level ?? match.level} · {titleCase(own?.role ?? match.role)}</span></div>
        </div>
        <div className="md-your-game"><span>Your contribution{own && <> · <b className={own.teamId === 100 ? "is-blue" : "is-red"}>{own.teamId === 100 ? "Blue" : "Red"} side</b></>}</span><strong>{own ? stat(own.kills) : match.kills}<i> / </i><em>{own ? stat(own.deaths) : match.deaths}</em><i> / </i>{own ? stat(own.assists) : match.assists}</strong>
          <div><span>{stat(own ? kda(own) : match.metrics[0], 2)} KDA</span><span>{stat(own ? own.cs : match.cs)} CS</span>{own?.score != null && <span className="md-hero-score">Score <b>{Math.round(own.score)}</b><small>/100</small></span>}</div>
        </div>
      </div>
      {facts.length > 0 && <Facts facts={facts} />}
    </section>;
  return <div className="md-page">
    <header className="md-context" aria-label="Match summary">
      <ChampionIcon id={match.championId} name={name(match.championId)} mode="desktop" />
      <strong>{name(match.championId)}</strong><b className={match.win ? "is-win" : "is-loss"}>{match.win ? "Victory" : "Defeat"}</b>
      <span>{queueLabels[match.queue] ?? match.queue}</span><span>{duration}</span>
      <time dateTime={new Date(match.createdAt).toISOString()} title={new Date(match.createdAt).toLocaleString()}>{new Date(match.createdAt).toLocaleDateString()}</time>
    </header>
    {error ? <Notice tone="error" action={<button className="fa-text-button" onClick={() => setAttempt(n => n + 1)}>Retry</button>}>{error}</Notice>
      : !detail ? <Skeleton label="Loading match details and player ranks" rows={5} height={66} /> : <>
        <div ref={tabBar} className="md-tabs" role="tablist" aria-label="Match views">{tabs.map((label, index) => <button key={label} id={`md-tab-${label}`} role="tab" tabIndex={tab === label ? 0 : -1} aria-selected={tab === label} aria-controls="md-panel" onClick={() => selectTab(label)} onKeyDown={event => {
          const next = event.key === "ArrowRight" ? (index + 1) % tabs.length : event.key === "ArrowLeft" ? (index + tabs.length - 1) % tabs.length : event.key === "Home" ? 0 : event.key === "End" ? tabs.length - 1 : null;
          if (next != null) { event.preventDefault(); selectTab(tabs[next]); document.getElementById(`md-tab-${tabs[next]}`)?.focus({ preventScroll: true }); }
        }}>{label}</button>)}</div>
        {detail.players.some(p => !p.rankLoaded) && <Notice tone={rankPending ? "info" : "warn"}>{rankPending ? "Loading ranks…" : "Some player ranks are unavailable."}</Notice>}
        {detail.players.length !== 10 && <Notice tone="warn">League Client returned {detail.players.length} of 10 players for this match. Some team statistics and performance scores are unavailable.</Notice>}
        <div id="md-panel" role="tabpanel" aria-labelledby={`md-tab-${tab}`}>
        {tab === "Overview" ? <div className="md-overview">{hero}<Comparison players={detail.players} teams={detail.teams} /></div>
        : tab === "Stats" ? <Stats players={detail.players} name={name} /> : tab === "Graphs" && detail.timeline ? <Timeline data={detail.timeline} players={detail.players} name={name} /> : <>
        {teamRows.map(({ team, players, total, best, average }) => {
          return <section className={`md-team ${team.teamId === 100 ? "is-blue" : "is-red"} ${team.win === true ? "is-win" : team.win === false ? "is-loss" : ""}`} key={team.teamId} aria-label={`${team.teamId === 100 ? "Blue" : "Red"} team`}>
            <header><strong>{team.win == null ? "Result unavailable" : team.win ? "Victory" : "Defeat"}</strong><span>{team.teamId === 100 ? "Blue side" : "Red side"}</span>
              <div className="md-objectives">{team.objectives.map(o => {
                return <span key={o.label} title={o.label} aria-label={`${o.kills} ${o.label}`}><ObjectiveIcon label={o.label} side={team.teamId} /><b>{o.kills}</b></span>;
              })}</div>
              <span className="md-team-total"><b>{stat(total.kills)} / {stat(total.deaths)} / {stat(total.assists)}</b> KDA <b>{compact(total.gold)}</b> gold</span>
            </header>
            <div className="md-team-scroll" role="region" aria-label={`${team.teamId === 100 ? "Blue" : "Red"} team ${tab.toLowerCase()}`} tabIndex={0}>
            {tab === "Scoreboard" && <div className="md-player md-scoreboard md-cols" aria-hidden><span /><span className="md-cols-player">Player</span><span>Items</span><span>K / D / A</span><span>CS</span><span>Damage</span><span>Score</span></div>}
            {players.map(p => <div className={`md-player${p.me ? " is-me" : ""} md-${tab.toLowerCase()}`} key={p.participantId}>
              {tab === "Scoreboard" && <Placement label={placementLabel(p.placement, team.win, p === best)} />}
              <div className="md-champion"><ChampionIcon id={p.championId} name={name(p.championId)} mode="desktop" />{p.level != null && <b>{p.level}</b>}</div>
              {tab === "Scoreboard" && <><div className="md-spells">{p.spells.map((id, i) => <SpellIcon id={id} key={i} mode="desktop" />)}</div><div className="md-perks">{[p.keystone, p.secondaryStyle].filter((id): id is number => !!id).map((id, i) => <Slot kind="rune" id={id} key={i} size={20} />)}</div></>}
              <div className="md-identity"><strong title={p.riotId || name(p.championId)}><Identity value={p.riotId || name(p.championId)} />{p.me && <small className="md-you">You</small>}</strong><small><Rank player={p} />{name(p.championId)}</small></div>
              {tab === "Scoreboard" ? <>
                <Items ids={p.items} />
                <div className="md-kda"><strong>{stat(p.kills)} / <em>{stat(p.deaths)}</em> / {stat(p.assists)}</strong><small>{stat(kda(p), 2)} KDA · {stat(participation(p, total.kills))}% KP</small></div>
                <div className="md-farm"><strong>{p.role === "support" ? stat(p.vision) : stat(p.cs != null && minutes > 0 ? p.cs / minutes : null, 1)}</strong><small>{p.role === "support" ? "vision" : `${stat(p.cs)} CS`}</small></div>
                <div className="md-damage"><span><b>{compact(p.damage)}</b> dmg</span>
                  <i style={{ "--damage": `${barWidth(p.damage, maxDamage)}%`, "--average": average == null ? undefined : `${barWidth(average, maxDamage)}%` } as CSSProperties} title={average == null ? undefined : `Team average ${compact(average)}`} /></div>
                <Score value={p.score} />
              </> : <>
                <div className="md-spells is-row" aria-label="Summoner spells">{p.spells.map((id, i) => <Slot kind="spell" id={id} key={i} size={26} />)}</div>
                <Runes player={p} /><Items ids={p.items} /></>}
            </div>)}</div>
          </section>;
        })}</>}
        </div>
        {(tab === "Scoreboard" || tab === "Loadout") && <p className="md-rank-note">Score: KDA 35%, damage 35%, CS 15%, vision 15%, vs this game’s best.</p>}
      </>}
  </div>;
}

function Identity({ value }: { value: string }) { const [name, tag] = splitRiotId(value); return <>{name}<span className="riot-tag">{tag}</span></>; }
function Rank({ player }: { player: Player }) {
  const label = rankLabel(player.rank, player.rankLoaded);
  const current = player.rank ? `Current ${player.rank.queue ?? "Solo/Duo"} rank` : "Current rank";
  return <span className="md-rank" title={current} aria-label={`${current}: ${label}`}>{player.rank && <RankCrest tier={crestSlug(player.rank.tier)} size={20} mode="desktop" />}{label}</span>;
}
/** Best performer of a team by placement, the MVP (winners) or ACE (losers). */
function bestOf(players: Player[]) {
  return [...players].filter(p => p.placement != null).sort((a, b) => a.placement! - b.placement! || a.participantId - b.participantId)[0];
}
function Placement({ label, long }: { label: string; long?: boolean }) {
  const pill = label === "MVP" || label === "ACE";
  return <span className={`md-placement${pill ? ` md-pill is-${label.toLowerCase()}` : ""}`}>{long && !pill ? `${label} place` : label}</span>;
}
/** Always seven fixed slots (six items and the trinket); missing ones draw as framed empties. */
function Items({ ids }: { ids: number[] }) { return <div className="md-items" aria-label="Final items">{Array.from({ length: ITEM_SLOTS }, (_, i) => <Slot kind="item" id={ids[i] ?? 0} key={i} className={i === ITEM_SLOTS - 1 ? "is-trinket" : undefined} />)}</div>; }
/** Keystone and primary tree, then the secondary tree, each group on its own. perks are the six chosen runes, four primary then two secondary. */
function Runes({ player }: { player: Player }) {
  if (!player.perks.length) return <small className="md-runes-missing">Runes unavailable</small>;
  const [keystone, ...primary] = player.perks.slice(0, 4);
  return <div className="md-runes" aria-label="Runes">
    <div className="md-rune-group"><Slot kind="rune" id={keystone} size={36} className="is-keystone" />{primary.map((id, i) => <Slot kind="rune" id={id} key={i} size={26} />)}</div>
    <div className="md-rune-group">{player.secondaryStyle ? <Slot kind="rune" id={player.secondaryStyle} size={26} className="is-tree" /> : null}{player.perks.slice(4, 6).map((id, i) => <Slot kind="rune" id={id} key={i} size={26} />)}</div>
  </div>;
}
function Score({ value }: { value: number | null }) {
  return <span className={`md-score is-${scoreTone(value)}`} title="Performance score relative to the match" aria-label={`Performance score ${value == null ? "unavailable" : `${Math.round(value)} out of 100`}`}>
    <svg viewBox="0 0 40 40" aria-hidden><circle cx="20" cy="20" r="17" /><circle cx="20" cy="20" r="17" pathLength="100" strokeDasharray={`${value ?? 0} 100`} /></svg><b>{value == null ? "–" : Math.round(value)}</b>
  </span>;
}
