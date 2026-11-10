import { Fragment, useEffect, useRef, useState, type CSSProperties, type PointerEvent } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { ChevronRight } from "lucide-react";
import { MatchupsIcon } from "./ui/icons";
import type { Account } from "../App";
import { ChampionIcon } from "../runes/ChampionIcon";
import { RankCrest } from "../runes/RankCrest";
import { RoleIcon } from "../runes/RoleIcon";
import { RuneIcon } from "../runes/RuneIcon";
import { SpellIcon } from "../runes/SpellIcon";
import { fmtAgo } from "../runes/types";
import { useChampionArt } from "./art";
import { benchKey, defaultQueue, deltaText, formatStat, meter, MIN_COMPARED, newestFirst, smoothPath, opponent, playTime, rankProgress, tierLabel, tiles, today, tone } from "./dashboardModel";
import type { AccountCard, Bench, Game, LpPoint, Mmr, RoleRecord, Snapshot } from "./types";
type ChampionRecord = Snapshot["summaries"][string]["champions"][number];
import { Slot } from "./ui/Slot";
import { Tooltip } from "./ui/Tooltip";
import { EmptyState, Skeleton } from "./ui/States";
import { fmtCompact } from "./ui/format";
export const queueLabels: Record<string, string> = { all: "All", solo: "Solo/Duo", flex: "Flex", normal: "Normal" };
const roleLabels: Record<string, string> = { top: "Top", jungle: "Jungle", mid: "Mid", adc: "Bot", support: "Support" };

export { crestForTier as crestSlug } from "./buildLogic";
import { crestForTier as crestSlug } from "./buildLogic";
import { LpGraph } from "./LpGraph";
import { MmrLine } from "./MmrLine";

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

export function RoleStrip({ roles }: { roles: RoleRecord[] }) {
  const most = Math.max(0, ...roles.map(r => r.games));
  return <div className="role-strip" aria-label="Win rate by role (estimated from spells, items and lane)">
    {Object.entries(roleLabels).map(([role, label]) => {
      const r = roles.find(r => r.role === role) ?? { role, games: 0, wins: 0 };
      return <Tooltip key={role} label={`${label}: ${r.wins}W ${r.games - r.wins}L`}><div className={!r.games ? "is-empty" : r.games === most ? "is-main" : ""}>
        <RoleIcon role={role} size={18} mode="desktop" />
        <b className={!r.games ? "" : pct(r.wins, r.games) >= 50 ? "up" : "down"}>{r.games ? `${pct(r.wins, r.games)}%` : "–"}</b>
        <small>{r.games} {r.games === 1 ? "game" : "games"}</small>
        <i style={{ "--rate": `${pct(r.wins, r.games)}%` } as CSSProperties} aria-hidden />
      </div></Tooltip>;
    })}
  </div>;
}
// Large stats (damage) as 22.4k so the big tile number fits.
const big = (value: number, digits: number, unit: string) => value >= 1000 ? fmtCompact(value) : formatStat(value, digits, unit);
const clock = (seconds: number) => `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;

type Props = {
  snapshot: Snapshot; phase: string; onOpenLive: () => void; onOpenChampion: (id: number, name: string) => void;
  onOpenMatch: (game: Game) => void;
  /** A game that just ended but is not in match history yet. */
  justEnded?: Game | null;
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
  const games = newestFirst(snapshot.games).filter(game => queue === "all" || game.queue === queue);
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
  const listed = props.justEnded && !snapshot.games.some(game => game.id === props.justEnded!.id) ? [props.justEnded, ...games] : games;
  useEffect(() => { document.querySelector(".match-row.is-last")?.scrollIntoView({ block: "nearest" }); }, []);
  return <div className={`dash-v2${stale ? " is-stale" : ""}`} onPointerMove={spotlight}>
    <div className="dash-hero">
    <div className="dash-top">
      <TodayCard games={snapshot.games} rank={snapshot.rank} history={snapshot.lpHistory} mmr={snapshot.mmr} live={inGamePhases.has(props.phase)} onOpenLive={props.onOpenLive} />
      <RankCard snapshot={snapshot} stale={stale} />
    </div>
    <Performance games={games} queue={queue} onQueue={setQueue} benches={benches} tier={tier} name={name} />
    </div>
    <div className="dash-columns">
      <section className="panel matches-panel" aria-labelledby="matches-title">
        <div className="panel-head"><h2 id="matches-title">Recent games</h2><small>{games.length} {games.length === 1 ? "game" : "games"} · {queue === "all" ? "all queues" : queueLabels[queue]}</small></div>
        {!games.length ? <NoMatches queue={queue} hasOthers={snapshot.games.length > 0} onShowAll={() => setQueue("all")} />
          : <ol className="match-list">{listed.map((game, i) => <Fragment key={game.id}>
            {(i === 0 || dayOf(listed[i - 1].createdAt) !== dayOf(game.createdAt)) && <DayHeader games={listed.filter(g => dayOf(g.createdAt) === dayOf(game.createdAt))} />}
            <MatchRow game={game} name={name} isLast={game.id === lastOpenedMatch} onOpen={() => openMatch(game)} />
          </Fragment>)}</ol>}
      </section>
      <section className="panel" aria-labelledby="champions-title">
        <div className="panel-head"><h2 id="champions-title">Champions &amp; roles</h2><small>{queueLabels[queue]}</small></div>
        {!summary.champions.length ? <p className="panel-empty">Play a game in this queue to see champions.</p> : <ul className="champ-perf">
          {summary.champions.map(c => <ChampCard key={c.id} c={c} name={name(c.id)} onOpen={() => onOpenChampion(c.id, name(c.id))} />)}
        </ul>}
        <RoleStrip roles={summary.roles} />
      </section>
    </div>
  </div>;
}

/** Tells the top panel where the pointer is, for the light that follows it (dashboard.css). */
function spotlight(event: PointerEvent<HTMLElement>) {
  const card = (event.target as Element).closest<HTMLElement>(".dash-hero");
  if (!card) return;
  const box = card.getBoundingClientRect();
  card.style.setProperty("--mx", `${event.clientX - box.left}px`);
  card.style.setProperty("--my", `${event.clientY - box.top}px`);
}

const dayOf = (at: number) => new Date(at).setHours(0, 0, 0, 0);

/** Heads each day's games in the list: the day and its record. LP stays on the Today card, which reads the recorded history. */
function DayHeader({ games }: { games: Game[] }) {
  const days = Math.round((dayOf(Date.now()) - dayOf(games[0].createdAt)) / 864e5);
  const wins = games.filter(g => g.win).length;
  return <li className="match-day">
    <strong>{days === 0 ? "Today" : days === 1 ? "Yesterday" : new Date(games[0].createdAt).toLocaleDateString([], { weekday: "long", month: "short", day: "numeric" })}</strong>
    <small>{wins}W {games.length - wins}L</small>
  </li>;
}

function ChampCard({ c, name, onOpen }: { c: ChampionRecord; name: string; onOpen: () => void }) {
  const [item, seen] = useSeen<HTMLLIElement>();
  const art = useChampionArt(seen ? c.id : null, "splash");
  const total = c.wins + c.losses;
  return <li ref={item}><Tooltip label={`Open ${name} build`}><button className="ui-row" onClick={onOpen}>
    {art && <img className="champ-art" src={art} alt="" aria-hidden />}
    <ChampionIcon id={c.id} mode="desktop" />
    <span className="champ-perf-copy"><strong>{name}</strong><small>{c.kda.toFixed(2)} KDA · {total} {total === 1 ? "game" : "games"}</small></span>
    <span className="champ-perf-rate"><b className={pct(c.wins, total) >= 50 ? "up" : "down"}>{pct(c.wins, total)}%</b><small>{c.wins}W {c.losses}L</small><i style={{ "--rate": `${pct(c.wins, total)}%` } as CSSProperties} aria-hidden /></span>
  </button></Tooltip></li>;
}

/** A stat's game-by-game line, oldest to newest; drawn only with three or more games. */
function Spark({ values }: { values: number[] }) {
  if (values.length < 3) return null;
  const min = Math.min(...values), span = Math.max(...values) - min || 1;
  const line = smoothPath(values.map((v, i) => ({ x: 100 * i / (values.length - 1), y: 3 + 22 * (1 - (v - min) / span) })));
  return <svg className="tile-spark" viewBox="0 0 100 28" preserveAspectRatio="none" aria-hidden>
    <path className="tile-spark-area" d={`${line} L100,28 L0,28 Z`} /><path d={line} />
  </svg>;
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
  return <EmptyState icon={<MatchupsIcon size={22} />} title={filtered ? `No ${queueLabels[queue]} games in your latest 20` : "No games yet"}
    action={filtered ? <button className="fa-text-button" onClick={onShowAll}>Show all queues</button> : undefined}>
    Games appear a minute after the end screen.
  </EmptyState>;
}

const inGamePhases = new Set(["GameStart", "InProgress", "Reconnect"]);

function TodayCard({ games, rank, history, mmr, live, onOpenLive }: { games: Game[]; rank: Snapshot["rank"]; history?: LpPoint[]; mmr?: Mmr | null; live: boolean; onOpenLive: () => void }) {
  const day = today(games, Date.now(), rank, history);
  const since = day.since == null ? "no games yet" : `since ${new Date(day.since).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}`;
  const none = day.games === 0;
  // Today's results, newest first; `since` is today's first game.
  const results = day.since == null ? [] : newestFirst(games).filter(g => g.createdAt >= day.since!).map(g => g.win);
  const run = results.findIndex(win => win !== results[0]);
  const streak = run < 0 ? results.length : run;
  return <section className="panel today-card" aria-label="Today">
    <div className="card-head"><h2>Today</h2><small>{since}</small>{live && <button className="fa-primary live-button" onClick={onOpenLive}>Open live game</button>}</div>
    <dl className="today-stats">
      <div><dt>{day.games === 1 ? "game" : "games"}</dt><dd>{day.games}</dd></div>
      <div><dt>wins–losses</dt><dd>{none ? "–" : <><span className="up">{day.wins}</span>–<span className="down">{day.losses}</span></>}</dd></div>
      <Tooltip label={day.lp == null ? "No Solo/Duo LP change could be worked out for today" : "Ranked Solo/Duo only, from your recorded LP"}><div>
        <dt>{day.lp != null && day.lp < 0 ? "LP lost" : "LP gained"}</dt><dd className={day.lp == null ? "" : day.lp >= 0 ? "up" : "down"}>{day.lp == null ? "–" : `${day.lp >= 0 ? "+" : "−"}${Math.abs(day.lp)}`}</dd></div></Tooltip>
      <div><dt>played</dt><dd>{none ? "–" : playTime(day.seconds)}</dd></div>
    </dl>
    {!none && <div className="today-form" aria-label={`Today's results, oldest first: ${[...results].reverse().map(win => win ? "win" : "loss").join(", ")}`}>
      {[...results].reverse().map((win, i) => <i key={i} className={win ? "" : "is-loss"} />)}
      {streak >= 2 && <small className={results[0] ? "up" : "down"}>{streak} {results[0] ? "win" : "loss"} streak</small>}
    </div>}
    {mmr && <MmrLine mmr={mmr} explain={`Swapper's estimate, not Riot's: the average Solo/Duo rank of the other players in your last ${mmr.games} ranked ${mmr.games === 1 ? "game" : "games"}, as recorded when each game started (current ranks for games before Swapper recorded them).`} />}
  </section>;
}

/** The full ranked emblem art for a tier slug, or null until (or unless) it loads. */
function useRankEmblem(tier: string) {
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    setUrl(null);
    if (tier !== "all") void invoke<string | null>("rank_emblem", { tier }).then(found => { if (live) setUrl(found); }).catch(() => {});
    return () => { live = false; };
  }, [tier]);
  return url;
}

function RankCard({ snapshot, stale }: { snapshot: Snapshot; stale: boolean }) {
  const rank = snapshot.rank;
  const emblem = useRankEmblem(crestSlug(rank?.tier));
  const progress = rank && rankProgress(rank);
  const total = rank ? rank.wins + rank.losses : 0;
  const queue = <small>Ranked Solo/Duo{stale && ` · saved ${fmtAgo(snapshot.fetchedAt)}`}</small>;
  if (!rank) return <section className="panel rank-card" aria-label="Ranked progress">
    <div className="card-head"><h2>Unranked</h2>{queue}</div>
    <p className="rank-empty">Finish placements to see progress.</p>
  </section>;
  return <section className="panel rank-card" data-tier={crestSlug(rank.tier)} aria-label="Ranked progress">
    <div className="rank-layout">
    <div className="rank-summary">
    {emblem && <span className="rank-emblem" style={{ backgroundImage: `url("${emblem}")` }} aria-hidden />}
    <div className="card-head">
      {!emblem && <RankCrest tier={crestSlug(rank.tier)} size={56} mode="desktop" />}
      {queue}<h2>{`${titleCase(rank.tier)} ${rank.division}`.trim()}</h2><b className="rank-lp">{rank.lp}<small>LP</small></b>
    </div>
    {total > 0 && <div className="rank-record" aria-label={`${pct(rank.wins, total)}% win rate, ${rank.wins} wins, ${rank.losses} losses`}>
      <strong className={rank.wins * 2 >= total ? "up" : "down"}>{pct(rank.wins, total)}%<small>WR</small></strong>
      <div className="rank-record-bar" aria-hidden><i style={{ width: `${100 * rank.wins / total}%` }} /></div>
      <span aria-hidden><b className="up">{rank.wins}W</b><b className="down">{rank.losses}L</b></span>
    </div>}
    <div className="rank-progress">
      {progress!.next && <div className="lp-bar" role="progressbar" aria-label={`${progress!.toNext} LP to ${progress!.next}`} aria-valuemin={0} aria-valuemax={100} aria-valuenow={progress!.lp}><i style={{ width: `${progress!.lp}%` }} /></div>}
      <p><span>{progress!.next ? `${progress!.toNext} LP to ${progress!.next}` : "Apex tier"}</span></p>
    </div>
    </div>
    <LpGraph history={snapshot.lpHistory} /></div>
  </section>;
}

function Performance({ games, queue, onQueue, benches, tier, name }: {
  games: Game[]; queue: string; onQueue: (queue: string) => void; benches: Record<string, Bench>; tier: string; name: (id: number) => string;
}) {
  const rows = tiles(games, benches, tierLabel(tier), name);
  const wins = games.filter(g => g.win).length;
  return <section className="panel performance" aria-labelledby="performance-title">
    <div className="performance-head">
      <h2 id="performance-title">Performance</h2>
      <div className="fa-seg" role="group" aria-label="Queue">
        {Object.entries(queueLabels).map(([value, label]) => <button key={value} aria-pressed={queue === value} onClick={() => onQueue(value)}>{label}</button>)}
      </div>
      <div className="wl-strip" aria-label={`${wins} wins, ${games.length - wins} losses, newest first`}>
        {games.map(g => <Tooltip key={g.id} label={`${g.win ? "Win" : "Loss"} · ${name(g.championId)} · ${g.kills}/${g.deaths}/${g.assists} · ${fmtAgo(g.createdAt)}`}><span className={g.win ? "is-win" : "is-loss"}>
          <ChampionIcon id={g.championId} mode="desktop" /></span></Tooltip>)}
        {games.length > 0 && <span className="wl-ring" style={{ "--rate": `${pct(wins, games.length)}%` } as CSSProperties} aria-hidden>{pct(wins, games.length)}</span>}
        <small>{wins}W {games.length - wins}L</small>
      </div>
    </div>
    <div className="tiles">{rows.map(tile => {
      const versus = tile.value != null ? tile.versus : null;
      const m = versus ? meter(tile.value!, versus.value) : 0;
      return <Tooltip key={tile.key} label={versus ? `You ${formatStat(tile.value!, tile.digits, tile.unit)} vs ${versus.label} ${formatStat(versus.value, tile.digits, tile.unit)} over the same ${versus.games} games` : undefined}>
      <article className={`tile tone-${versus ? tone(tile.value!, versus.value) : "none"}`}>
        <small className="tile-label">{tile.label}</small>
        <strong className="tile-value">{tile.value == null ? "–" : big(tile.value, tile.digits, tile.unit)}</strong>
        {versus && <i className={`tile-meter${m < 0 ? " is-neg" : ""}`} style={{ "--w": `${Math.abs(m) * 50}%` } as CSSProperties} aria-hidden />}
        <p className="tile-foot">{versus ? deltaText({ ...tile, versus }) : tile.value == null ? `Needs ${MIN_COMPARED} games`
          : games.length < MIN_COMPARED ? `Needs ${MIN_COMPARED} games to compare` : "Not enough comparable games"}</p>
        <Spark values={[...tile.series].reverse()} />
      </article>
      </Tooltip>;
    })}</div>
  </section>;
}

const ITEM_SLOTS = 7;

export function MatchRow({ game, name, isLast, onOpen }: { game: Game; name: (id: number) => string; isLast: boolean; onOpen: () => void }) {
  const ratio = (game.kills + game.assists) / Math.max(1, game.deaths);
  const [row, seen] = useSeen<HTMLLIElement>();
  const opp = opponent(game);
  const csDiff = opp ? game.cs - opp.cs : null;
  const art = useChampionArt(seen ? game.championId : null, "splash");
  return <li ref={row} className={`match-row ${game.win ? "is-win" : "is-loss"}${isLast ? " is-last" : ""}`} onClick={onOpen}>
    {art && <img className="match-art" src={art} alt="" aria-hidden />}
    <Tooltip label={`Open ${name(game.championId)} match details`}><button className="match-champ" onClick={event => { event.stopPropagation(); onOpen(); }} aria-label={`Open ${name(game.championId)} match details`}>
      <ChampionIcon id={game.championId} mode="desktop" />
      {game.level > 0 && <b>{game.level}</b>}
    </button></Tooltip>
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
    <Tooltip label={opp ? `Lane opponent: ${name(opp.championId)}` : "Lane opponent unclear"}><div className="match-opp">
      {opp ? <>{seen && <ChampionIcon id={opp.championId} mode="desktop" />}<span><small>vs {name(opp.championId)}</small><b className={csDiff! >= 0 ? "up" : "down"}>{csDiff! >= 0 ? "+" : "−"}{Math.abs(csDiff!)} CS</b></span></> : <small>–</small>}
    </div></Tooltip>
    <div className="match-items" aria-label="Items">
      {!game.items.length ? <small>Refresh to load items</small>
        : Array.from({ length: ITEM_SLOTS }, (_, i) => <Slot key={i} kind="item" id={game.items[i] ?? 0} pending={!seen} className={i === ITEM_SLOTS - 1 ? "is-trinket" : undefined} />)}
    </div>
    <Tooltip label="Rank among all 10 players by KDA, damage, CS and vision"><div className={`placement${game.placement === 1 ? " is-mvp" : ""}`}>
      {game.placement ? <><b>#{game.placement}</b><small>{game.placement === 1 ? "MVP" : "place"}</small></> : <small>–</small>}
    </div></Tooltip>
    <ChevronRight className="match-chevron" size={18} aria-hidden />
  </li>;
}
