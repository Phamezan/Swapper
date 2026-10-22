import { useEffect, useRef, useState, type CSSProperties } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { ChevronRight, Swords } from "lucide-react";
import type { Account } from "../App";
import { ChampionIcon } from "../runes/ChampionIcon";
import { RankCrest } from "../runes/RankCrest";
import { RoleIcon } from "../runes/RoleIcon";
import { RuneIcon } from "../runes/RuneIcon";
import { SpellIcon } from "../runes/SpellIcon";
import { fmtAgo } from "../runes/types";
import { benchKey, defaultQueue, deltaText, formatStat, insights, lpTrend, MIN_COMPARED, opponent, playTime, rankProgress, sparkline, tierLabel, tiles, today, tone, type WinRecord } from "./dashboardModel";
import type { AccountCard, Bench, Game, Snapshot } from "./types";
import { Slot } from "./ui/Slot";
import { EmptyState, Skeleton } from "./ui/States";
import { fmtCompact } from "./ui/format";
export const queueLabels: Record<string, string> = { all: "All", solo: "Solo/Duo", flex: "Flex", normal: "Normal" };
const roleLabels: Record<string, string> = { top: "Top", jungle: "Jungle", mid: "Mid", adc: "Bot", support: "Support" };

export { crestForTier as crestSlug } from "./buildLogic";
import { crestForTier as crestSlug } from "./buildLogic";

export function titleCase(text: string) {
  return text.charAt(0) + text.slice(1).toLowerCase();
}

export function splitRiotId(riotId: string): [string, string] {
  const at = riotId.lastIndexOf("#");
  return at > 0 ? [riotId.slice(0, at), riotId.slice(at)] : [riotId, ""];
}

/** True once the element has come within a screen of the viewport. Match rows
 *  load their ~10 icons only then, so opening the Dashboard decodes a third
 *  of them (each icon is a data URL held by the page). */
function useSeen<T extends Element>() {
  const ref = useRef<T>(null);
  const [seen, setSeen] = useState(false);
  useEffect(() => {
    const node = ref.current;
    if (!node || seen) return;
    const observer = new IntersectionObserver(entries => {
      if (entries.some(entry => entry.isIntersecting)) { setSeen(true); observer.disconnect(); }
    }, { rootMargin: "200px 0px" });
    observer.observe(node);
    return () => observer.disconnect();
  }, [seen]);
  return [ref, seen] as const;
}

const pct = (wins: number, games: number) => (games ? Math.round(100 * wins / games) : 0);
// Large stats (damage) as 22.4k so the big tile number fits.
const big = (value: number, digits: number, unit: string) => value >= 1000 ? fmtCompact(value) : formatStat(value, digits, unit);
const clock = (seconds: number) => `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;

type Props = {
  snapshot: Snapshot; phase: string; onOpenLive: () => void; onOpenChampion: (id: number, name: string) => void;
  onOpenMatch: (game: Game) => void;
  /** True while only the saved snapshot is available (League Client closed). */
  stale?: boolean;
  /** The sidebar podium owns accounts now; main.tsx may still pass these. */
  region?: string | null; accounts?: Account[]; activeId?: string | null; cards?: Record<string, AccountCard>; busy?: boolean; onSwitch?: (id: string) => void;
};

/** The match opened last, so the list can mark it when the user comes back. Lives
 *  outside the component because opening a match unmounts the Dashboard. */
let lastOpenedMatch: number | null = null;

export function Dashboard(props: Props) {
  const { snapshot, onOpenChampion, stale = false } = props;
  const [queue, setQueue] = useState(() => defaultQueue(snapshot.games));
  const [benches, setBenches] = useState<Record<string, Bench>>({});
  const games = snapshot.games.filter(game => queue === "all" || game.queue === queue);
  const summary = snapshot.summaries[queue] ?? snapshot.summaries.all;
  const name = (id: number) => snapshot.championNames[id] ?? `Champion ${id}`;
  const tier = snapshot.rank?.tier ?? "";
  const picks = [...new Set(snapshot.games.filter(g => g.role).map(benchKey))].sort().join(",");
  const openMatch = (game: Game) => { lastOpenedMatch = game.id; props.onOpenMatch(game); };
  useEffect(() => {
    if (!isTauri() || !picks) return;
    let live = true;
    const list = picks.split(",").map(key => { const [id, role] = key.split("|"); return [Number(id), role]; });
    invoke<Record<string, Bench>>("dashboard_benchmarks", { picks: list, tier }).then(next => { if (live) setBenches(next); }).catch(() => {});
    return () => { live = false; };
  }, [picks, tier]);
  useEffect(() => { document.querySelector(".match-row.is-last")?.scrollIntoView({ block: "nearest" }); }, []);
  return <div className={`dash-v2${stale ? " is-stale" : ""}`}>
    <div className="dash-top">
      <TodayCard games={snapshot.games} live={inGamePhases.has(props.phase)} onOpenLive={props.onOpenLive} />
      <RankCard snapshot={snapshot} stale={stale} />
    </div>
    <Performance games={games} queue={queue} onQueue={setQueue} benches={benches} tier={tier} name={name} />
    <div className="dash-columns">
      <section className="panel matches-panel" aria-labelledby="matches-title">
        <div className="panel-head"><h2 id="matches-title">Recent games</h2><small>{games.length} {games.length === 1 ? "game" : "games"} · {queue === "all" ? "all queues" : queueLabels[queue]}</small></div>
        {!games.length ? <NoMatches queue={queue} hasOthers={snapshot.games.length > 0} onShowAll={() => setQueue("all")} />
          : <ol className="match-list">{games.map(game => <MatchRow key={game.id} game={game} name={name} isLast={game.id === lastOpenedMatch} onOpen={() => openMatch(game)} />)}</ol>}
      </section>
      <section className="panel" aria-labelledby="champions-title">
        <div className="panel-head"><h2 id="champions-title">Champions &amp; roles</h2><small>{queueLabels[queue]}</small></div>
        {!summary.champions.length ? <p className="panel-empty">Play a game in this queue to see champions.</p> : <ul className="champ-perf">
          {summary.champions.map(c => {
            const total = c.wins + c.losses;
            return <li key={c.id}><button className="ui-row" onClick={() => onOpenChampion(c.id, name(c.id))} title={`Open ${name(c.id)} build`}>
              <ChampionIcon id={c.id} name={name(c.id)} mode="desktop" />
              <span className="champ-perf-copy"><strong>{name(c.id)}</strong><small>{c.kda.toFixed(2)} KDA · {total} {total === 1 ? "game" : "games"}</small></span>
              <span className="champ-perf-rate"><b className={pct(c.wins, total) >= 50 ? "up" : "down"}>{pct(c.wins, total)}%</b><small>{c.wins}W {c.losses}L</small><i style={{ "--rate": `${pct(c.wins, total)}%` } as CSSProperties} aria-hidden /></span>
            </button></li>;
          })}
        </ul>}
        <div className="role-strip" aria-label="Win rate by role (estimated from spells, items and lane)">
          {summary.roles.map(r => <div key={r.role} className={r.games ? "" : "is-empty"} title={`${roleLabels[r.role]}: ${r.wins}W ${r.games - r.wins}L`}>
            <RoleIcon role={r.role} size={18} mode="desktop" />
            <b className={!r.games ? "" : pct(r.wins, r.games) >= 50 ? "up" : "down"}>{r.games ? `${pct(r.wins, r.games)}%` : "–"}</b>
            <small>{r.games} {r.games === 1 ? "game" : "games"}</small>
          </div>)}
        </div>
      </section>
    </div>
    <Insights games={games} name={name} />
  </div>;
}

/** Placeholder shaped like the Dashboard while the first snapshot loads. */
export function DashboardSkeleton() {
  return <div className="dash-v2">
    <div className="dash-top"><Skeleton label="Loading today" rows={1} height={118} /><Skeleton label="Loading rank" rows={1} height={118} /></div>
    <Skeleton label="Loading performance" rows={1} height={170} />
    <Skeleton label="Loading match history" rows={5} height={76} />
  </div>;
}

function NoMatches({ queue, hasOthers, onShowAll }: { queue: string; hasOthers: boolean; onShowAll: () => void }) {
  const filtered = queue !== "all" && hasOthers;
  return <EmptyState icon={<Swords size={22} />} title={filtered ? `No ${queueLabels[queue]} games in your latest 20` : "No games yet"}
    action={filtered ? <button className="fa-text-button" onClick={onShowAll}>Show all queues</button> : undefined}>
    Games appear a minute after the end screen.
  </EmptyState>;
}

const inGamePhases = new Set(["GameStart", "InProgress", "Reconnect"]);

function TodayCard({ games, live, onOpenLive }: { games: Game[]; live: boolean; onOpenLive: () => void }) {
  const day = today(games, Date.now());
  const since = day.since == null ? "no games yet" : `since ${new Date(day.since).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}`;
  const none = day.games === 0;
  return <section className="panel today-card" aria-label="Today">
    <div className="card-head"><h2>Today</h2><small>{since}</small>{live && <button className="fa-primary live-button" onClick={onOpenLive}>Open live game</button>}</div>
    <dl className="today-stats">
      <div><dt>{day.games === 1 ? "game" : "games"}</dt><dd>{day.games}</dd></div>
      <div><dt>wins–losses</dt><dd>{none ? "–" : <><span className="up">{day.wins}</span>–<span className="down">{day.losses}</span></>}</dd></div>
      <div title={day.lp == null ? "No LP change was recorded for today's games" : "Recorded Solo/Duo LP changes"}>
        <dt>{day.lp != null && day.lp < 0 ? "LP lost" : "LP gained"}</dt><dd className={day.lp == null ? "" : day.lp >= 0 ? "up" : "down"}>{day.lp == null ? "–" : `${day.lp >= 0 ? "+" : "−"}${Math.abs(day.lp)}`}</dd></div>
      <div><dt>played</dt><dd>{none ? "–" : playTime(day.seconds)}</dd></div>
    </dl>
  </section>;
}

function RankCard({ snapshot, stale }: { snapshot: Snapshot; stale: boolean }) {
  const rank = snapshot.rank;
  const trend = lpTrend(snapshot.lpHistory);
  const progress = rank && rankProgress(rank);
  const total = rank ? rank.wins + rank.losses : 0;
  return <section className="panel rank-card" aria-label="Ranked progress">
    <div className="card-head">{rank ? <>
      <RankCrest tier={crestSlug(rank.tier)} size={26} mode="desktop" />
      <h2>{`${titleCase(rank.tier)} ${rank.division}`.trim()}</h2><b className="rank-lp">{rank.lp} LP</b></> : <h2>Unranked</h2>}
      <small>Ranked Solo/Duo{stale && ` · saved ${fmtAgo(snapshot.fetchedAt)}`}</small></div>
    {rank ? <div className="rank-body">
      <div className="rank-progress">
        {progress!.next && <div className="lp-bar" role="progressbar" aria-label={`${progress!.toNext} LP to ${progress!.next}`} aria-valuemin={0} aria-valuemax={100} aria-valuenow={progress!.lp}><i style={{ width: `${progress!.lp}%` }} /></div>}
        <p><span>{progress!.next ? `${progress!.toNext} LP to ${progress!.next}` : "Apex tier"}</span><span>Season {rank.wins}W {rank.losses}L{total > 0 && `, ${pct(rank.wins, total)}%`}</span></p>
      </div>
      {trend && <figure className="lp-trend" title="Solo/Duo LP Swapper recorded">
        <svg viewBox="0 0 150 40" aria-hidden><polyline points={sparkline(trend.scores, 150, 40)} /></svg>
        <figcaption><span>{trend.games > 0 ? `${trend.games} ${trend.games === 1 ? "game" : "games"} ago` : "earlier"}</span><span>now</span></figcaption>
      </figure>}
    </div> : <p className="rank-empty">Finish placements to see progress.</p>}
  </section>;
}

function Performance({ games, queue, onQueue, benches, tier, name }: {
  games: Game[]; queue: string; onQueue: (queue: string) => void; benches: Record<string, Bench>; tier: string; name: (id: number) => string;
}) {
  const rows = tiles(games, benches, tierLabel(tier), name);
  const oldestFirst = [...games].reverse();
  const wins = games.filter(g => g.win).length;
  return <section className="panel performance" aria-labelledby="performance-title">
    <div className="performance-head">
      <h2 id="performance-title">Performance</h2>
      <div className="fa-seg" role="group" aria-label="Queue">
        {Object.entries(queueLabels).map(([value, label]) => <button key={value} aria-pressed={queue === value} onClick={() => onQueue(value)}>{label}</button>)}
      </div>
      <div className="wl-strip" aria-label={`${wins} wins, ${games.length - wins} losses, oldest first`}>
        {oldestFirst.map(g => <span key={g.id} className={g.win ? "is-win" : "is-loss"} title={`${g.win ? "Win" : "Loss"} · ${name(g.championId)} · ${g.kills}/${g.deaths}/${g.assists} · ${fmtAgo(g.createdAt)}`}>
          <ChampionIcon id={g.championId} name={name(g.championId)} mode="desktop" /></span>)}
        <small>{wins}W {games.length - wins}L</small>
      </div>
    </div>
    <div className="tiles">{rows.map(tile => {
      const versus = tile.value != null ? tile.versus : null;
      return <article key={tile.key} className={`tile tone-${versus ? tone(tile.value!, versus.value) : "none"}`}
        title={versus ? `You ${formatStat(tile.value!, tile.digits, tile.unit)} vs ${versus.label} ${formatStat(versus.value, tile.digits, tile.unit)} over the same ${versus.games} games` : undefined}>
        <small className="tile-label">{tile.label}</small>
        <strong className="tile-value">{tile.value == null ? "–" : big(tile.value, tile.digits, tile.unit)}</strong>
        <p className="tile-foot">{versus ? deltaText({ ...tile, versus }) : tile.value == null ? `Needs ${MIN_COMPARED} games`
          : games.length < MIN_COMPARED ? `Needs ${MIN_COMPARED} games to compare` : "Not enough comparable games"}</p>
      </article>;
    })}</div>
  </section>;
}

function Insights({ games, name }: { games: Game[]; name: (id: number) => string }) {
  const lines = games.length >= 5 ? insights(games, name) : [];
  if (!lines.length) return null;
  return <section aria-label="Win-rate patterns">
    {lines.length > 0 && <ul className="insights">{lines.map(line => {
      const better = line.yes.rate >= line.no.rate;
      return <li key={line.topic + line.condition} className="insight">
        <p className="insight-topic">{line.topic}<small>Win rate</small></p>
        <div className="insight-sides">
          <InsightSide label={line.condition} record={line.yes} up={better} />
          <InsightSide label={line.other} record={line.no} up={!better} />
        </div>
      </li>;
    })}</ul>}
  </section>;
}

function InsightSide({ label, record, up }: { label: string; record: WinRecord; up: boolean }) {
  return <div className={`insight-side ${up ? "up" : "down"}`}>
    <span>{label}</span>
    <b>{record.rate}<i>%</i></b>
    <small>{record.wins}W {record.games - record.wins}L</small>
    <i className="insight-meter" style={{ "--rate": `${record.rate}%` } as CSSProperties} aria-hidden />
  </div>;
}

const ITEM_SLOTS = 7;

function MatchRow({ game, name, isLast, onOpen }: { game: Game; name: (id: number) => string; isLast: boolean; onOpen: () => void }) {
  const ratio = (game.kills + game.assists) / Math.max(1, game.deaths);
  const [row, seen] = useSeen<HTMLLIElement>();
  const opp = opponent(game);
  const csDiff = opp ? game.cs - opp.cs : null;
  return <li ref={row} className={`match-row ${game.win ? "is-win" : "is-loss"}${isLast ? " is-last" : ""}`} onClick={onOpen}>
    <button className="match-champ" onClick={event => { event.stopPropagation(); onOpen(); }} title={`Open ${name(game.championId)} match details`}>
      <ChampionIcon id={game.championId} name={name(game.championId)} mode="desktop" />
      {game.level > 0 && <b>{game.level}</b>}
    </button>
    <div className="match-result">
      <p><strong>{game.win ? "Victory" : "Defeat"}</strong>{game.lpChange != null &&
        <b className={game.lpChange >= 0 ? "up" : "down"}>{game.lpChange >= 0 ? "+" : "−"}{Math.abs(game.lpChange)} LP</b>}</p>
      <small>{queueLabels[game.queue]} · {fmtAgo(game.createdAt)} · {clock(game.duration)}</small>
      {isLast && <em className="match-seen">Last opened</em>}
    </div>
    <div className="match-kda">
      <strong>{game.kills} <i>/</i> <em>{game.deaths}</em> <i>/</i> {game.assists}</strong>
      <small className={ratio >= 3 ? "up" : ratio < 1.5 ? "down" : ""}>{ratio.toFixed(2)} KDA</small>
    </div>
    <div className="match-loadout" aria-label="Spells and keystone">
      {seen && game.spells.map((id, i) => <SpellIcon key={i} id={id} mode="desktop" />)}
      {seen && game.keystone > 0 && <RuneIcon id={game.keystone} mode="desktop" className="match-keystone" />}
    </div>
    <div className="match-cs"><strong>{game.cs || "–"} CS</strong><small>{game.metrics[2] == null ? "per game" : `${game.metrics[2].toFixed(1)} / min`}</small></div>
    <div className="match-opp" title={opp ? `Lane opponent: ${name(opp.championId)}` : "Lane opponent unclear"}>
      {opp ? <>{seen && <ChampionIcon id={opp.championId} name={name(opp.championId)} mode="desktop" />}<span><small>vs {name(opp.championId)}</small><b className={csDiff! >= 0 ? "up" : "down"}>{csDiff! >= 0 ? "+" : "−"}{Math.abs(csDiff!)} CS</b></span></> : <small>–</small>}
    </div>
    <div className="match-items" aria-label="Items">
      {!game.items.length ? <small>Refresh to load items</small>
        : Array.from({ length: ITEM_SLOTS }, (_, i) => <Slot key={i} kind="item" id={game.items[i] ?? 0} pending={!seen} className={i === ITEM_SLOTS - 1 ? "is-trinket" : undefined} />)}
    </div>
    <div className={`placement${game.placement === 1 ? " is-mvp" : ""}`} title="Rank among all 10 players by KDA, damage, CS and vision">
      {game.placement ? <><b>#{game.placement}</b><small>{game.placement === 1 ? "MVP" : "place"}</small></> : <small>–</small>}
    </div>
    <ChevronRight className="match-chevron" size={18} aria-hidden />
  </li>;
}
