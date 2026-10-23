import { useId, useMemo, useState, type KeyboardEvent, type PointerEvent } from "react";
import { RankCrest } from "../runes/RankCrest";
import { crestForTier as crestSlug } from "./buildLogic";
import { lpGraph } from "./dashboardModel";
import type { LpPoint } from "./types";
import "./lp-graph.css";

const W = 400, H = 210;

const signedLp = (lp: number) => <b><i className={lp >= 0 ? "up" : "down"} aria-hidden>{lp === 0 ? "–" : lp > 0 ? "▲" : "▼"}</i><span className="lp-graph-sr">{lp < 0 ? "Lost" : "Gained"} </span>{Math.abs(lp)} <small>LP</small></b>;
const tone = (tier: string) => `var(--ui-tier-${crestSlug(tier)})`;
const date = (at: number) => new Date(at).toLocaleDateString([], { day: "numeric", month: "short" });

function Rank({ point }: { point: LpPoint }) {
  return <span className="lp-graph-rank" style={{ color: tone(point.tier) }}>
    <RankCrest tier={crestSlug(point.tier)} size={22} mode="desktop" /><span className="lp-graph-sr">{point.tier} </span>{point.division ? `${point.division} – ` : ""}{point.lp} LP
  </span>;
}

/** Solo/Duo LP over the last 30 days, from the LP Swapper recorded. Hover shows each recorded point. */
export function LpGraph({ history }: { history: LpPoint[] | undefined }) {
  const graph = useMemo(() => lpGraph(history, Date.now(), W, H), [history]);
  const [hover, setHover] = useState<number | null>(null);
  // Keep fragment ids simple and unique when multiple charts share a document.
  const id = `lp${useId().replace(/[^a-zA-Z0-9]/g, "")}`;
  if (!graph) return null;
  const { points, peak } = graph;
  const last = points[points.length - 1];
  const active = hover == null ? null : points[hover];
  const pick = (event: PointerEvent<HTMLDivElement>) => {
    const box = event.currentTarget.getBoundingClientRect();
    const x = (event.clientX - box.left) / box.width * W;
    setHover(points.reduce((best, p, i) => Math.abs(p.x - x) < Math.abs(points[best].x - x) ? i : best, 0));
  };
  const move = (event: KeyboardEvent<HTMLDivElement>) => {
    if (!["ArrowLeft", "ArrowRight", "Home", "End", "Escape"].includes(event.key)) return;
    event.preventDefault();
    if (event.key === "Escape") { setHover(null); return; }
    setHover(event.key === "Home" ? 0 : event.key === "End" ? points.length - 1
      : Math.max(0, Math.min(points.length - 1, (hover ?? points.length - 1) + (event.key === "ArrowLeft" ? -1 : 1))));
  };
  const at = (p: { x: number; y: number }) => ({ left: `${100 * p.x / W}%`, top: `${100 * p.y / H}%` });
  return <figure className="lp-graph" aria-label={`Solo/Duo LP over the last 30 days. Peak ${peak.tier} ${peak.division} ${peak.lp} LP.`}>
    <figcaption className="lp-graph-gains" title="Changes in the LP Swapper recorded. Recording gaps may cover multiple games.">
      <span>Last 30d {graph.month == null ? "–" : signedLp(graph.month)}</span>
      <span>Last 7d {graph.week == null ? "–" : signedLp(graph.week)}</span>
    </figcaption>
    <div className="lp-graph-plot" tabIndex={0} role="group" aria-label="Recorded LP history. Use Left and Right arrows to explore, Home and End to jump, Escape to dismiss."
      onPointerMove={pick} onPointerLeave={() => setHover(null)} onFocus={() => setHover(points.length - 1)} onBlur={() => setHover(null)} onKeyDown={move}>
      {graph.guides.map(guide => <span key={guide.y} className={`lp-graph-guide${guide.tier ? " is-tier" : ""}`} style={{ top: `${100 * guide.y / H}%` }} title={guide.tier ? `${guide.tier} IV · 0 LP` : guide.label} aria-hidden>
        {guide.tier ? <RankCrest tier={crestSlug(guide.tier)} size={26} mode="desktop" /> : <i>{guide.label}</i>}
      </span>)}
      <svg viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" aria-hidden>
        <defs>
          {/* Hard stops at tier lines: the line takes the color of the tier it is in. */}
          <linearGradient id={`${id}-tier`} gradientUnits="userSpaceOnUse" x1="0" y1="0" x2="0" y2={H}>
            {graph.tiers.flatMap(t => [<stop key={`${t.tier}a`} offset={t.from} style={{ stopColor: tone(t.tier) }} />, <stop key={`${t.tier}b`} offset={t.to} style={{ stopColor: tone(t.tier) }} />])}
          </linearGradient>
          <linearGradient id={`${id}-fade`} x1="0" y1="0" x2="0" y2="1">
            <stop offset="0" stopColor="white" stopOpacity=".14" />
            <stop offset="1" stopColor="white" stopOpacity="0" />
          </linearGradient>
          <mask id={`${id}-fill`}><rect width={W} height={H} fill={`url(#${id}-fade)`} /></mask>
        </defs>
        <path className="lp-graph-area" d={`${graph.path} L${last.x.toFixed(1)},${H} L0,${H} Z`} fill={`url(#${id}-tier)`} mask={`url(#${id}-fill)`} />
        <path className="lp-graph-line" d={graph.path} stroke={`url(#${id}-tier)`} />
        {active && <line className="lp-graph-cross" x1={active.x} x2={active.x} y1={0} y2={H} />}
      </svg>
      {/* HTML, not SVG: the stretched viewBox would squash a circle into an oval. */}
      <i className="lp-graph-dot" style={{ ...at(active ?? last), color: tone((active ?? last).point.tier) }} aria-hidden />
      <div className="lp-graph-tip" role="status" aria-atomic="true" hidden={!active} style={active ? { ...at(active),
        left: `clamp(0px, calc(${100 * active.x / W}% + ${active.x > W / 2 ? "-156px" : "16px"}), calc(100% - 140px))`,
        transform: `translateY(${active.y > H / 2 ? "calc(-100% - 16px)" : "16px"})` } : undefined}>
        {active && <><small>{date(active.point.at)}</small>
        <Rank point={active.point} />
        <small>{active.games > 0 ? `${active.games} ${active.games === 1 ? "game" : "games"}` : "Recorded rank"}</small></>}
      </div>
    </div>
    <div className="lp-graph-dates" aria-hidden><span>{date(points[0].point.at)}</span><span>{date(last.point.at)}</span></div>
    <p className="lp-graph-peak"><span>Peak</span><Rank point={peak} /><small>Last 30d</small></p>
  </figure>;
}
