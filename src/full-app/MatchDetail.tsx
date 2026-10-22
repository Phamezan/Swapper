import { useEffect, useState, type CSSProperties, type ReactNode } from "react";
import { Bug, Castle, Crown, Eye, Flame, type LucideIcon } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ChampionIcon } from "../runes/ChampionIcon";
import { SpellIcon } from "../runes/SpellIcon";
import { RankCrest } from "../runes/RankCrest";
import { fmtAgo } from "../runes/types";
import { crestSlug, queueLabels, splitRiotId, titleCase } from "./Dashboard";
import { useChampionArt } from "./art";
import { barWidth, percent, placementLabel, scoreTone, teamAverageDamage, teamTotals } from "./MatchDetailModel";
import type { Game, MatchDetailPlayer as Player, MatchDetailView, Snapshot } from "./types";
import { Slot } from "./ui/Slot";
import { Notice, Skeleton } from "./ui/States";
import { fmtCompact, fmtCount } from "./ui/format";
import "./MatchDetail.css";

const stat = (value: number | null | undefined, digits = 0) => value == null ? "–" : digits ? value.toFixed(digits) : fmtCount(value);
const compact = (value: number | null | undefined) => value == null ? "–" : fmtCompact(value);
const kda = (p: Player) => p.kills == null || p.deaths == null || p.assists == null ? null : (p.kills + p.assists) / Math.max(1, p.deaths);
const OBJECTIVE_ICONS: Record<string, LucideIcon> = { Dragons: Flame, Heralds: Eye, Grubs: Bug, Barons: Crown, Towers: Castle };
const ITEM_SLOTS = 7;
const participation = (p: Player, total: number | null) => percent(p.kills != null && p.assists != null ? p.kills + p.assists : null, total);

export default function MatchDetail({ game, names }: { game: Game; names: Snapshot["championNames"] }) {
  const [detail, setDetail] = useState<MatchDetailView | null>(null);
  const [error, setError] = useState("");
  const [attempt, setAttempt] = useState(0);
  const [rankPending, setRankPending] = useState(true);
  const [tab, setTab] = useState("Scoreboard");
  const splash = useChampionArt(game.championId, "splash");
  useEffect(() => {
    let live = true; setDetail(null); setError(""); setRankPending(true);
    const stop = listen<MatchDetailView>("match_detail", ({ payload }) => { if (live && payload.game.id === game.id) setDetail(payload); });
    void stop.then(() => live ? invoke<MatchDetailView>("full_app_match_detail", { gameId: game.id }) : undefined)
      .then(value => { if (live && value) setDetail(value); }).catch(reason => { if (live) setError(String(reason)); })
      .finally(() => { if (live) setRankPending(false); });
    return () => { live = false; void stop.then(unlisten => unlisten()); };
  }, [game.id, attempt]);
  const match = detail?.game ?? game;
  const own = detail?.players.find(p => p.me);
  const ownTeam = detail?.players.filter(p => p.teamId === own?.teamId) ?? [];
  const totals = teamTotals(ownTeam);
  const ownLabel = own ? placementLabel(own.placement, detail?.teams.find(t => t.teamId === own.teamId)?.win ?? null, own === bestOf(ownTeam)) : "–";
  const name = (id: number) => names[id] ?? `Champion ${id}`;
  const duration = `${Math.floor(match.duration / 60)}:${String(match.duration % 60).padStart(2, "0")}`;
  const minutes = match.duration / 60;
  const stats: [ReactNode, string][] = [
    [<>{own ? stat(own.kills) : match.kills} / <em>{own ? stat(own.deaths) : match.deaths}</em> / {own ? stat(own.assists) : match.assists}</>, `${stat(own ? kda(own) : match.metrics[0], 2)} KDA`],
    [`${stat(own ? participation(own, totals.kills) : match.metrics[1])}${(own ? participation(own, totals.kills) : match.metrics[1]) == null ? "" : "%"}`, "Kill participation"],
    [stat(own?.cs != null && minutes > 0 ? own.cs / minutes : match.metrics[2], 1), `CS/min · ${stat(own ? own.cs : match.cs)} total`],
    [compact(own?.damage), `Damage · ${stat(own ? percent(own.damage, totals.damage) : null)}% of team`],
    [stat(own?.vision), "Vision score"],
  ];
  return <div className="md-page">
    <section className={`md-header ${match.win ? "is-win" : "is-loss"}`} aria-label="Your match performance">
      {splash && <img className="md-art" src={splash} alt="" draggable={false} loading="lazy" />}
      <div className="md-header-main"><div className="md-champion"><ChampionIcon id={match.championId} name={name(match.championId)} mode="desktop" /><b>{own?.level ?? match.level}</b></div>
        <div className="md-spells">{(own?.spells ?? match.spells).map((id, i) => <SpellIcon id={id} key={i} mode="desktop" />)}</div>
        {(own?.keystone ?? match.keystone) > 0 && <Slot kind="rune" id={own?.keystone ?? match.keystone} size={34} />}
        <div className="md-header-copy"><div><strong>{own?.riotId ? <Identity value={own.riotId} /> : "Your match"}</strong>{own && <Rank player={own} />}</div>
          <p>{name(match.championId)} · {queueLabels[match.queue] ?? match.queue} · {duration} · {fmtAgo(match.createdAt)}</p></div>
        <Score value={own?.score ?? null} />
        <div className="md-result"><h2 className={match.win ? "up" : "down"}>{match.win ? "Victory" : "Defeat"}</h2>{ownLabel !== "–" && <Placement label={ownLabel} long />}</div>
      </div>
      <div className="md-stat-strip">{stats.map(([value, label]) => <div key={label}><strong>{value}</strong><small>{label}</small></div>)}</div>
    </section>
    {error ? <Notice tone="error" action={<button className="fa-text-button" onClick={() => setAttempt(n => n + 1)}>Retry</button>}>{error}</Notice>
      : !detail ? <Skeleton label="Loading match details and player ranks" rows={5} height={66} /> : <>
        <div className="md-tabs" role="tablist" aria-label="Match views">{["Scoreboard", "Loadout", "Details"].map((label, index, tabs) => <button key={label} id={`md-tab-${label}`} role="tab" tabIndex={tab === label ? 0 : -1} aria-selected={tab === label} aria-controls="md-panel" onClick={() => setTab(label)} onKeyDown={event => {
          const next = event.key === "ArrowRight" ? (index + 1) % tabs.length : event.key === "ArrowLeft" ? (index + tabs.length - 1) % tabs.length : event.key === "Home" ? 0 : event.key === "End" ? tabs.length - 1 : null;
          if (next != null) { event.preventDefault(); setTab(tabs[next]); document.getElementById(`md-tab-${tabs[next]}`)?.focus(); }
        }}>{label}</button>)}</div>
        {detail.players.some(p => !p.rankLoaded) && <Notice tone={rankPending ? "info" : "warn"}>{rankPending ? "Loading current Solo/Duo ranks…" : "Some current Solo/Duo ranks are unavailable. Available match statistics are shown below."}</Notice>}
        {detail.players.length !== 10 && <Notice tone="warn">League Client returned {detail.players.length} of 10 players for this match. Some team statistics and performance scores are unavailable.</Notice>}
        <div id="md-panel" role="tabpanel" aria-labelledby={`md-tab-${tab}`}>
        {detail.teams.map(team => {
          const players = detail.players.filter(p => p.teamId === team.teamId);
          const total = teamTotals(players);
          const best = bestOf(players);
          const average = teamAverageDamage(players);
          const maxDamage = Math.max(1, ...detail.players.map(p => p.damage ?? 0));
          const maxOf = (pick: (p: Player) => number | null) => Math.max(1, ...detail.players.map(p => pick(p) ?? 0));
          return <section className={`md-team ${team.teamId === 100 ? "is-blue" : "is-red"} ${team.win === true ? "is-win" : team.win === false ? "is-loss" : ""}`} key={team.teamId} aria-label={`${team.teamId === 100 ? "Blue" : "Red"} team`}>
            <header><strong>{team.win == null ? "Result unavailable" : team.win ? "Victory" : "Defeat"}</strong><span>{team.teamId === 100 ? "Blue side" : "Red side"}</span>
              <div className="md-objectives">{team.objectives.map(o => {
                const Icon = OBJECTIVE_ICONS[o.label];
                return <span key={o.label} title={o.label} aria-label={`${o.kills} ${o.label}`}>{Icon ? <Icon size={16} aria-hidden /> : o.label}<b>{o.kills}</b></span>;
              })}</div>
              <span className="md-team-total"><b>{stat(total.kills)} / {stat(total.deaths)} / {stat(total.assists)}</b> KDA <b>{compact(total.gold)}</b> gold</span>
            </header>
            {tab === "Scoreboard" && <div className="md-player md-scoreboard md-cols" aria-hidden><span /><span className="md-cols-player">Player</span><span>Items</span><span>K / D / A</span><span>CS</span><span>Damage</span><span>Score</span></div>}
            {players.map(p => <div className={`md-player${p.me ? " is-me" : ""} md-${tab.toLowerCase()}`} key={p.participantId}>
              {tab === "Scoreboard" && <Placement label={placementLabel(p.placement, team.win, p === best)} />}
              <div className="md-champion"><ChampionIcon id={p.championId} name={name(p.championId)} mode="desktop" />{p.level != null && <b>{p.level}</b>}</div>
              {tab === "Scoreboard" && <><div className="md-spells">{p.spells.map((id, i) => <SpellIcon id={id} key={i} mode="desktop" />)}</div><div className="md-perks">{[p.keystone, p.secondaryStyle].filter((id): id is number => !!id).map((id, i) => <Slot kind="rune" id={id} key={i} size={20} />)}</div></>}
              <div className="md-identity"><strong><Identity value={p.riotId || name(p.championId)} />{p.me && <small className="md-you">You</small>}</strong><small><Rank player={p} />{name(p.championId)}</small></div>
              {tab === "Scoreboard" ? <>
                <Items ids={p.items} />
                <div className="md-kda"><strong>{stat(p.kills)} / <em>{stat(p.deaths)}</em> / {stat(p.assists)}</strong><small>{stat(kda(p), 2)} KDA · {stat(participation(p, total.kills))}% KP</small></div>
                <div className="md-farm"><strong>{p.role === "support" ? stat(p.vision) : stat(p.cs != null && minutes > 0 ? p.cs / minutes : null, 1)}</strong><small>{p.role === "support" ? "vision" : `${stat(p.cs)} CS`}</small></div>
                <div className="md-damage"><span><b>{compact(p.damage)}</b> dmg</span>
                  <i style={{ "--damage": `${barWidth(p.damage, maxDamage)}%`, "--average": average == null ? undefined : `${barWidth(average, maxDamage)}%` } as CSSProperties} title={average == null ? undefined : `Team average ${compact(average)}`} /></div>
                <Score value={p.score} />
              </> : tab === "Loadout" ? <>
                <div className="md-spells is-row" aria-label="Summoner spells">{p.spells.map((id, i) => <Slot kind="spell" id={id} key={i} size={26} />)}</div>
                <Runes player={p} /><Items ids={p.items} /></>
                : <div className="md-extra">{[["Gold", p.gold, maxOf(q => q.gold)], ["Damage taken", p.damageTaken, maxOf(q => q.damageTaken)], ["Vision", p.vision, maxOf(q => q.vision)], ["Wards placed", p.wardsPlaced, maxOf(q => q.wardsPlaced)], ["Wards cleared", p.wardsKilled, maxOf(q => q.wardsKilled)]].map(([label, value, max]) => <span key={String(label)}><strong>{compact(value as number | null)}</strong><small>{label}</small><i style={{ "--w": `${barWidth(value as number | null, max as number)}%` } as CSSProperties} /></span>)}</div>}
            </div>)}
          </section>;
        })}</div>
        <p className="md-rank-note">Score: KDA 35%, champion damage 35%, CS 15%, vision 15%, relative to this match’s best. Ranks are current Solo/Duo ranks.</p>
      </>}
  </div>;
}

function Identity({ value }: { value: string }) { const [name, tag] = splitRiotId(value); return <>{name}<span className="riot-tag">{tag}</span></>; }
function Rank({ player }: { player: Player }) { return <span className="md-rank" title="Current Solo/Duo rank">{player.rank ? <><RankCrest tier={crestSlug(player.rank.tier)} size={17} mode="desktop" />{titleCase(player.rank.tier)} {player.rank.division}</> : player.rankLoaded ? "Unranked" : "Rank unavailable"}</span>; }
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
