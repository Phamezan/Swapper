import { useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { HoverCard } from "radix-ui";
import { ChampionIcon } from "../../runes/ChampionIcon";
import { ObjectiveIcon } from "../ObjectiveIcon";
import { groupKillEvents, resolveTimelineEvents, timelineChart, type MatchPlayer, type ResolvedTimelineEvent, type Timeline as TimelineData } from "../MatchDetailModel";
import { fmtCompact, fmtCount } from "../ui/format";

const time = (ms: number) => `${Math.floor(ms / 60000)}:${String(Math.floor(ms / 1000) % 60).padStart(2, "0")}`;
const side = (teamId: number) => teamId === 100 ? "is-blue" : "is-red";
const lead = (value: number) => value === 0 ? "Teams even" : `${value > 0 ? "Blue" : "Red"} +${fmtCount(Math.abs(value))} gold`;
const personal = (event: ResolvedTimelineEvent) => event.kind === "kill" && (event.killer.me || event.victim.me);
const eventLabel = (event: ResolvedTimelineEvent, name: (id: number) => string) => event.kind === "kill"
  ? `${event.killer.me ? "You" : name(event.killer.championId)} killed ${event.victim.me ? "you" : name(event.victim.championId)}`
  : `${event.teamId === 100 ? "Blue" : "Red"} team took ${event.label.toLowerCase()}`;

function EventContent({ event, name }: { event: ResolvedTimelineEvent; name: (id: number) => string }) {
  return event.kind === "objective" ? <><ObjectiveIcon label={event.objective} side={event.teamId} /><b>{event.label}</b><span>{event.teamId === 100 ? "Blue" : "Red"} team</span></>
    : <><span className={`md-kill-portrait ${side(event.killer.teamId)}`}><ChampionIcon id={event.killer.championId} name={name(event.killer.championId)} mode="desktop" /></span><span aria-hidden>→</span><span className={`md-kill-portrait ${side(event.victim.teamId)}`}><ChampionIcon id={event.victim.championId} name={name(event.victim.championId)} mode="desktop" /></span><span>{event.victim.me ? "Your death" : event.killer.me ? "Your kill" : "Champion kill"}</span></>;
}

export function Timeline({ data, players, name }: { data: TimelineData; players: MatchPlayer[]; name: (id: number) => string }) {
  const chart = useMemo(() => timelineChart(data), [data]);
  const events = useMemo(() => resolveTimelineEvents(chart?.events ?? [], players), [chart, players]);
  const clusters = useMemo(() => groupKillEvents(events), [events]);
  const [selected, setSelected] = useState<number | null>(null);
  const [inspecting, setInspecting] = useState(false);
  const [filter, setFilter] = useState("All");
  const closeTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => () => clearTimeout(closeTimer.current), []);
  const showReading = () => { clearTimeout(closeTimer.current); setInspecting(true); };
  const hideReading = () => { clearTimeout(closeTimer.current); closeTimer.current = setTimeout(() => setInspecting(false), 120); };
  if (!chart) return null;
  const active = chart.points[selected ?? chart.points.length - 1];
  const line = chart.points.map(p => `${p.x},${p.y}`).join(" ");
  const filtered = events.filter(e => filter === "All" || (filter === "Kills" ? e.kind === "kill" : e.kind === "objective"));
  const minuteEvents = events.filter(e => Math.floor(e.timestamp / 60000) === Math.floor(active.timestamp / 60000));
  const ownTeam = players.find(p => p.me)?.teamId;
  const inspectAt = (timestamp: number) => {
    const next = chart.points.findIndex(p => p.timestamp > timestamp);
    setSelected(next === -1 ? chart.points.length - 1 : Math.max(0, next - 1));
    showReading();
  };
  return <section className="md-timeline ui-card" aria-label="Gold difference over time" onKeyDown={e => { if (e.key === "Escape") setInspecting(false); }}>
    <header className="md-view-heading"><div><h2>Gold over time</h2><p>Blue team − red team · {chart.points.length} recorded frames · Hover or inspect with the frame selector</p></div><div className="md-chart-reading"><b className={active.goldDiff === 0 ? "" : side(active.goldDiff > 0 ? 100 : 200)}>{lead(active.goldDiff)}</b><span>At {time(active.timestamp)}</span></div></header>
    <div className="md-chart-legend"><span className="is-blue">Blue advantage{ownTeam === 100 ? " · You" : ""}</span><span className="is-red">Red advantage{ownTeam === 200 ? " · You" : ""}</span><span>Portraits · champion kills</span></div>
    <div className="md-chart-wrap">
      <div className="md-chart-axis"><span>+{fmtCompact(chart.limit)}</span><span>Even</span><span>−{fmtCompact(chart.limit)}</span></div>
      <div className="md-chart" style={{ "--cursor": `${active.x}%` } as CSSProperties} onPointerMove={e => {
        if (e.pointerType === "touch") return;
        const box = e.currentTarget.getBoundingClientRect();
        const x = Math.max(0, Math.min(100, (e.clientX - box.left) / box.width * 100));
        const nearest = chart.points.reduce((best, p, i) => Math.abs(p.x - x) < Math.abs(chart.points[best].x - x) ? i : best, 0);
        setSelected(nearest); showReading();
      }} onPointerLeave={hideReading}>
        <svg viewBox="0 0 100 100" preserveAspectRatio="none" aria-hidden>
          <defs><clipPath id="md-chart-blue"><rect width="100" height="50" /></clipPath><clipPath id="md-chart-red"><rect y="50" width="100" height="50" /></clipPath>
            <linearGradient id="md-fill-blue" x1="0" x2="0" y1="0" y2="1"><stop stopColor="var(--md-blue)" stopOpacity=".3" /><stop offset="1" stopColor="var(--md-blue)" stopOpacity=".03" /></linearGradient>
            <linearGradient id="md-fill-red" x1="0" x2="0" y1="0" y2="1"><stop stopColor="var(--md-red)" stopOpacity=".03" /><stop offset="1" stopColor="var(--md-red)" stopOpacity=".3" /></linearGradient>
          </defs>
          {[0, 25, 50, 75, 100].map(y => <line key={y} x1="0" x2="100" y1={y} y2={y} className={y === 50 ? "md-zero-line" : "md-grid-line"} />)}
          {[0, 25, 50, 75, 100].map(x => <line key={x} x1={x} x2={x} y1="0" y2="100" className="md-grid-line" />)}
          <polygon points={`0,50 ${line} 100,50`} clipPath="url(#md-chart-blue)" className="md-area-blue" /><polygon points={`0,50 ${line} 100,50`} clipPath="url(#md-chart-red)" className="md-area-red" />
          <polyline points={line} clipPath="url(#md-chart-blue)" className="md-line-blue" /><polyline points={line} clipPath="url(#md-chart-red)" className="md-line-red" />
        </svg>
        <i className={`md-chart-cursor${inspecting ? " is-active" : ""}`} aria-hidden />
        {filter !== "Kills" && events.filter(e => e.kind === "objective").map((event, i) => <button className={`md-chart-event ${side(event.teamId)}`} key={i} aria-label={`${time(event.timestamp)} · ${eventLabel(event, name)}`} style={{left: `${event.x}%`, top: `${event.y}%`}} onClick={() => inspectAt(event.timestamp)} onFocus={() => inspectAt(event.timestamp)} onBlur={hideReading}><ObjectiveIcon label={event.objective} side={event.teamId} /></button>)}
        <div className="md-kill-track" aria-label="Champion kills on the timeline">{filter !== "Objectives" && clusters.map((cluster, i) => {
          const featured = cluster.events.find(e => e.victim.me) ?? cluster.events.find(personal) ?? cluster.events[0];
          return <button className={`md-kill-marker ${side(featured.victim.teamId)}${cluster.events.some(personal) ? " is-personal" : ""}`} key={i} style={{left: `${(cluster.timestamp - chart.start) / chart.duration * 100}%`}} aria-label={`${time(cluster.timestamp)} · ${cluster.events.map(e => eventLabel(e, name)).join("; ")}`} onClick={() => inspectAt(cluster.timestamp)} onFocus={() => inspectAt(cluster.timestamp)} onBlur={hideReading}>
            <ChampionIcon id={featured.victim.championId} name={name(featured.victim.championId)} mode="desktop" />{cluster.events.length > 1 && <b>{cluster.events.length}</b>}{cluster.events.some(personal) && <span>You</span>}
          </button>;
        })}</div>
        <HoverCard.Root open={inspecting}>
          <HoverCard.Trigger asChild><span className={`md-chart-point ${active.goldDiff === 0 ? "" : side(active.goldDiff > 0 ? 100 : 200)}`} style={{left: `${active.x}%`, top: `${active.y}%`}} aria-hidden /></HoverCard.Trigger>
          <HoverCard.Portal><HoverCard.Content className="md-timeline-popover" side="top" sideOffset={16} collisionPadding={16} onPointerEnter={showReading} onPointerLeave={hideReading} onEscapeKeyDown={() => setInspecting(false)}>
            <div role="tooltip" id="md-minute-tooltip">
              <header><time>{time(active.timestamp)}</time><span>Recorded frame</span></header>
              <strong className={active.goldDiff === 0 ? "" : side(active.goldDiff > 0 ? 100 : 200)}>{lead(active.goldDiff)}</strong>
              <dl><div className="is-blue"><dt>Blue team{ownTeam === 100 ? " · You" : ""}</dt><dd>{active.blueGold == null ? "Unavailable" : fmtCount(active.blueGold)}</dd></div><div className="is-red"><dt>Red team{ownTeam === 200 ? " · You" : ""}</dt><dd>{active.redGold == null ? "Unavailable" : fmtCount(active.redGold)}</dd></div></dl>
              <div className="md-minute-events"><p>Events · {Math.floor(active.timestamp / 60000)}:00–{Math.floor(active.timestamp / 60000)}:59</p>{minuteEvents.length ? minuteEvents.map((event, i) => <div className={`md-minute-event ${side(event.teamId)}`} key={i} aria-label={eventLabel(event, name)}><EventContent event={event} name={name} /></div>) : <small>No events in this minute.</small>}</div>
            </div>
          </HoverCard.Content></HoverCard.Portal>
        </HoverCard.Root>
      </div>
    </div>
    <div className="md-chart-times">{[0, .25, .5, .75, 1].map(f => <span key={f}>{time(chart.start + chart.duration * f)}</span>)}</div>
    <label className="md-chart-select">Recorded frame<input type="range" min={0} max={chart.points.length - 1} value={selected ?? chart.points.length - 1} onFocus={showReading} onBlur={hideReading} onChange={e => { setSelected(Number(e.target.value)); showReading(); }} aria-label="Inspect recorded minute" aria-describedby={inspecting ? "md-minute-tooltip" : undefined} aria-valuetext={`${time(active.timestamp)}: ${lead(active.goldDiff)}`} /></label>
    <p className="md-chart-note">The line joins recorded frames. Icons mark objectives; portraits mark deaths, grouped within 30 seconds. A green edge marks your kills or deaths.</p>
    <div className="md-event-heading"><h3>Match events <small>{filtered.length}</small></h3><div className="md-event-filters" role="group" aria-label="Filter match events">{["All", "Objectives", "Kills"].map(label => <button key={label} aria-pressed={filter === label} onClick={() => { setFilter(label); setInspecting(false); }}>{label}</button>)}</div></div>
    <div className="md-event-list" aria-label="Match timeline">{filtered.length ? filtered.map((event, i) => <button key={i} className={`${side(event.teamId)}${personal(event) ? " is-personal" : ""}`} onClick={() => inspectAt(event.timestamp)} onBlur={() => setInspecting(false)} aria-label={`${time(event.timestamp)} · ${eventLabel(event, name)}`}>
      <time>{time(event.timestamp)}</time><EventContent event={event} name={name} />
    </button>) : <p>No {filter === "All" ? "events" : filter.toLowerCase()} were returned.</p>}</div>
  </section>;
}
