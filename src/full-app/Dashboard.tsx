import { useEffect, useRef, useState, type CSSProperties } from "react";
import { ChampionIcon } from "../runes/ChampionIcon";
import { ItemIcon } from "../runes/ItemIcon";
import { RankCrest } from "../runes/RankCrest";
import { RoleIcon } from "../runes/RoleIcon";
import { RuneIcon } from "../runes/RuneIcon";
import { SpellIcon } from "../runes/SpellIcon";
import { fmtAgo } from "../runes/types";
import { useChampionArt } from "./art";
import { sparkArea, sparkline } from "./sparkline";
import type { Game, Metric, Snapshot } from "./types";

const metricLabels = ["KDA", "Kill participation", "CS / min", "Damage / min", "Damage share", "Vision / min"];
const metricDigits = [2, 0, 1, 0, 0, 2];
const metricUnit = ["", "%", "", "", "%", ""];
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
const fmt = (value: number, i: number) => `${value.toFixed(metricDigits[i])}${metricUnit[i]}`;
const clock = (seconds: number) => `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;

type Props = { snapshot: Snapshot; region: string | null; onOpenChampion: (id: number, name: string) => void };

export function Dashboard({ snapshot, region, onOpenChampion }: Props) {
  const [queue, setQueue] = useState("all");
  const games = snapshot.games.filter(game => queue === "all" || game.queue === queue);
  const summary = snapshot.summaries[queue];
  const name = (id: number) => snapshot.championNames[id] ?? `Champion ${id}`;
  return <div className="dash-grid">
      <Hero snapshot={snapshot} region={region} featured={summary.champions[0]?.id ?? games[0]?.championId} queue={queue} onQueue={setQueue} metrics={summary.metrics} sampled={games.length} />
      <section className="panel matches-panel" aria-labelledby="matches-title">
        <div className="panel-head"><h2 id="matches-title">Match history</h2><small>{games.length} {games.length === 1 ? "game" : "games"} · {queue === "all" ? "all queues" : queueLabels[queue]}</small></div>
        {!games.length ? <p className="panel-empty">No {queueLabels[queue]} games in your latest 20 matches.</p>
          : <ol className="match-list">{games.map(game => <MatchRow key={game.id} game={game} name={name(game.championId)} onOpen={() => onOpenChampion(game.championId, name(game.championId))} />)}</ol>}
      </section>
    <aside className="dash-side">
      <RankCard rank={snapshot.rank} />
      <section className="panel" aria-labelledby="champions-title">
        <div className="panel-head"><h2 id="champions-title">Champion performance</h2><small>{queueLabels[queue]}</small></div>
        {!summary.champions.length ? <p className="panel-empty">Play a game to see champion results.</p> : <ul className="champ-perf">
          {summary.champions.map(c => {
            const total = c.wins + c.losses;
            return <li key={c.id}><button onClick={() => onOpenChampion(c.id, name(c.id))} title={`Open ${name(c.id)} build`}>
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
    </aside>
  </div>;
}

function Hero({ snapshot, region, featured, queue, onQueue, metrics, sampled }: {
  snapshot: Snapshot; region: string | null; featured: number | undefined; queue: string;
  onQueue: (queue: string) => void; metrics: Metric[]; sampled: number;
}) {
  const splash = useChampionArt(featured, "splash");
  const [gameName, tag] = splitRiotId(snapshot.summoner.riotId);
  return <section className="hero" aria-label="Summoner">
    {splash && <img className="hero-art" src={splash} alt="" draggable={false} />}
    <div className="hero-top">
      <div className="hero-avatar">
        {snapshot.summoner.icon ? <img src={snapshot.summoner.icon} alt="" /> : <span aria-hidden />}
        <b title="Summoner level">{snapshot.summoner.level}</b>
      </div>
      <div className="hero-name">
        <h2><span className="riot-name">{gameName}</span><span className="riot-tag">{tag}</span></h2>
        <p>{region && <span className="chip">{region}</span>}<span className="hero-sub">{sampled} games · latest 20 matches</span></p>
      </div>
      <div className="fa-seg" role="group" aria-label="Queue">
        {Object.entries(queueLabels).map(([value, label]) => <button key={value} aria-pressed={queue === value} onClick={() => onQueue(value)}>{label}</button>)}
      </div>
    </div>
    <div className="stat-grid">{metrics.map((metric, i) => <StatCard key={metricLabels[i]} index={i} metric={metric} />)}</div>
    <p className="hero-note">Averages per game. Change compares your latest 5 games with this queue’s average; the line shows your latest 10.</p>
  </section>;
}

function StatCard({ metric, index }: { metric: Metric; index: number }) {
  const trend = metric.delta === null ? "flat" : metric.delta >= 0 ? "up" : "down";
  const gradient = `spark-${index}`;
  return <article className={`stat-card trend-${trend}`}>
    <small>{metricLabels[index]}</small>
    <strong>{metric.average === null ? "–" : <>{metric.average.toFixed(metricDigits[index])}<i>{metricUnit[index]}</i></>}</strong>
    <span className="delta-chip" title="Latest 5 games vs this queue’s average">
      {metric.delta === null ? "No team data" : <>{metric.delta >= 0 ? "▲" : "▼"} {fmt(Math.abs(metric.delta), index)}</>}
    </span>
    {metric.points.length > 1 && <svg viewBox="0 0 100 28" preserveAspectRatio="none" role="img" aria-label={`${metricLabels[index]}, latest ${metric.points.length} games`}>
      <defs><linearGradient id={gradient} x1="0" x2="0" y1="0" y2="1"><stop offset="0" stopColor="currentColor" stopOpacity=".32" /><stop offset="1" stopColor="currentColor" stopOpacity="0" /></linearGradient></defs>
      <path d={sparkArea(metric.points)} fill={`url(#${gradient})`} />
      <polyline points={sparkline(metric.points)} fill="none" stroke="currentColor" strokeWidth="1.6" vectorEffect="non-scaling-stroke" strokeLinejoin="round" />
    </svg>}
  </article>;
}

function RankCard({ rank }: { rank: Snapshot["rank"] }) {
  const games = rank ? rank.wins + rank.losses : 0;
  return <section className={`panel rank-card${rank ? "" : " is-unranked"}`} data-tier={(rank?.tier ?? "").toLowerCase()} aria-label="Ranked Solo/Duo">
    <div className="rank-emblem"><RankCrest tier={crestSlug(rank?.tier)} size={76} mode="desktop" /></div>
    <div className="rank-copy">
      <small>Ranked Solo/Duo</small>
      <strong>{rank ? `${titleCase(rank.tier)} ${rank.division}`.trim() : "Unranked"}</strong>
      {rank ? <p><b>{rank.lp} LP</b> {rank.wins}W {rank.losses}L</p> : <p>Finish placements to get a rank.</p>}
    </div>
    {rank && <div className="rank-rate">
      <b className={pct(rank.wins, games) >= 50 ? "up" : "down"}>{pct(rank.wins, games)}%</b>
      <i style={{ "--rate": `${pct(rank.wins, games)}%` } as CSSProperties} aria-hidden />
    </div>}
  </section>;
}

function MatchRow({ game, name, onOpen }: { game: Game; name: string; onOpen: () => void }) {
  const ratio = (game.kills + game.assists) / Math.max(1, game.deaths);
  const [row, seen] = useSeen<HTMLLIElement>();
  return <li ref={row} className={`match-row ${game.win ? "is-win" : "is-loss"}`}>
    <div className="match-meta">
      <strong>{game.win ? "Victory" : "Defeat"}</strong>
      <span>{queueLabels[game.queue]}{game.lpChange != null &&
        <b className={game.lpChange >= 0 ? "up" : "down"}> {game.lpChange >= 0 ? "+" : "−"}{Math.abs(game.lpChange)} LP</b>}</span>
      <small>{fmtAgo(game.createdAt)} · {clock(game.duration)}</small>
    </div>
    <button className="match-champ" onClick={onOpen} title={`Open ${name} build`}>
      <ChampionIcon id={game.championId} name={name} mode="desktop" />
      {game.level > 0 && <b>{game.level}</b>}
    </button>
    <div className="match-loadout" aria-label="Spells and keystone">
      {seen && game.spells.map((id, i) => <SpellIcon key={i} id={id} mode="desktop" />)}
      {seen && game.keystone > 0 && <RuneIcon id={game.keystone} mode="desktop" className="match-keystone" />}
    </div>
    <div className="match-kda">
      <strong>{game.kills} <i>/</i> <em>{game.deaths}</em> <i>/</i> {game.assists}</strong>
      <small className={ratio >= 3 ? "up" : ratio < 1.5 ? "down" : ""}>{ratio.toFixed(2)} KDA</small>
    </div>
    <div className="match-cs"><strong>{game.cs || "–"}</strong><small>{game.metrics[2] === null ? "CS" : `${game.metrics[2].toFixed(1)} / min`}</small></div>
    <div className="match-items" aria-label="Items">
      {!game.items.length ? <small>Refresh to load items</small>
        : game.items.map((id, i) => seen ? <ItemIcon key={i} id={id} mode="desktop" className={i === 6 ? "is-trinket" : undefined} /> : <span key={i} className={i === 6 ? "is-trinket" : undefined} />)}
    </div>
    <div className={`placement${game.placement === 1 ? " is-mvp" : ""}`} title="Rank among all 10 players by KDA, damage, CS and vision">
      {game.placement ? <><b>#{game.placement}</b><small>{game.placement === 1 ? "MVP" : "place"}</small></> : <small>–</small>}
    </div>
  </li>;
}
