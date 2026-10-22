import { useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { HoverCard } from "radix-ui";
import { ObjectiveIcon } from "../ObjectiveIcon";
import { filterTimelineEvents, mapPosition, personalKill, resolveTimelineEvents, timelineChart, timelineLanes, type EventFilter, type MatchPlayer, type ResolvedTimelineEvent, type TimelineKill, type Timeline as TimelineData } from "../MatchDetailModel";
import { fmtCompact, fmtCount } from "../ui/format";
import { EventContent, eventLabel, side, time } from "./GraphEvents";
import { KillMap } from "./KillMap";

const lead = (gold: number, timestamp: number) => `${gold === 0 ? "Teams even" : `${gold > 0 ? "Blue" : "Red"} ahead by ${fmtCount(Math.abs(gold))}`} at ${time(timestamp)}`;
type EventInspection = {events: ResolvedTimelineEvent[]; x: number; y: number};

export function Timeline({ data, players, name }: { data: TimelineData; players: MatchPlayer[]; name: (id: number) => string }) {
  const chart = useMemo(() => timelineChart(data), [data]);
  const events = useMemo<ResolvedTimelineEvent[]>(() => resolveTimelineEvents(chart?.events ?? [], players), [chart, players]);
  const [selected, setSelected] = useState<number | null>(null);
  const [inspecting, setInspecting] = useState(false);
  const [eventInspection, setEventInspection] = useState<EventInspection | null>(null);
  const [participant, setParticipant] = useState<number | null>(null);
  const [filter, setFilter] = useState<EventFilter>("Key moments");
  const [expanded, setExpanded] = useState(false);
  const closeTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => () => clearTimeout(closeTimer.current), []);
  const visibleEvents = useMemo(() => filterTimelineEvents(events, "All", participant), [events, participant]);
  const lanes = useMemo(() => timelineLanes(visibleEvents), [visibleEvents]);
  const positionedKills = useMemo(() => events.filter((event): event is TimelineKill => event.kind === "kill" && mapPosition(event.position) != null), [events]);
  const mapKills = useMemo(() => positionedKills.filter(event => participant == null || event.killerId === participant || event.victimId === participant), [positionedKills, participant]);
  const filtered = useMemo(() => filterTimelineEvents(events, filter, participant), [events, filter, participant]);
  const active = chart?.points[Math.min(selected ?? chart.points.length - 1, chart.points.length - 1)];
  const minuteEvents = useMemo(() => events.filter(event => Math.floor(event.timestamp / 60000) === Math.floor((active?.timestamp ?? 0) / 60000)), [events, active]);
  const line = useMemo(() => chart?.points.map(point => `${point.x},${point.y}`).join(" "), [chart]);
  if (!chart || !active) return null;
  const keepOpen = () => clearTimeout(closeTimer.current);
  const close = () => { keepOpen(); setInspecting(false); setEventInspection(null); };
  const hide = () => { keepOpen(); closeTimer.current = setTimeout(close, 120); };
  const showFrame = () => { keepOpen(); setEventInspection(null); setInspecting(true); };
  const inspectEvents = (selection: ResolvedTimelineEvent[], target: HTMLElement) => {
    keepOpen(); setInspecting(false);
    const box = target.getBoundingClientRect();
    setEventInspection({events: selection, x: box.left + box.width / 2, y: box.top});
  };
  const selectPlayer = (id: number | null) => { setParticipant(id); setExpanded(false); close(); };
  const ownTeam = players.find(player => player.me)?.teamId;
  return <section className="md-timeline ui-card" aria-label="Gold difference over time" onKeyDown={event => { if (event.key === "Escape") close(); }}>
    <div className={`md-graphs-layout${positionedKills.length ? " has-map" : ""}`}>
      <div className="md-gold-panel">
        <header className="md-view-heading"><div><h2>Gold difference</h2><p>Blue team − red team · {chart.points.length} recorded frames</p></div>
          <div className="md-chart-reading" aria-live="off"><b className={active.goldDiff === 0 ? "" : side(active.goldDiff > 0 ? 100 : 200)}>{lead(active.goldDiff, active.timestamp)}</b></div>
        </header>
        <div className="md-chart-legend"><span className="is-blue">Blue advantage{ownTeam === 100 ? " · Your team" : ""}</span><span className="is-red">Red advantage{ownTeam === 200 ? " · Your team" : ""}</span></div>
        <div className="md-chart-wrap">
          <div className="md-chart-axis"><span>+{fmtCompact(chart.limit)}</span><span>Even</span><span>−{fmtCompact(chart.limit)}</span></div>
          <div className="md-chart" style={{"--cursor": `${active.x}%`} as CSSProperties} onPointerMove={event => {
            if (event.pointerType === "touch") return;
            const box = event.currentTarget.getBoundingClientRect();
            const x = Math.max(0, Math.min(100, (event.clientX - box.left) / box.width * 100));
            const nearest = chart.points.reduce((best, point, i) => Math.abs(point.x - x) < Math.abs(chart.points[best].x - x) ? i : best, 0);
            setSelected(nearest); showFrame();
          }} onPointerLeave={hide}>
            <svg viewBox="0 0 100 100" preserveAspectRatio="none" aria-hidden>
              <defs><clipPath id="md-chart-blue"><rect width="100" height="50" /></clipPath><clipPath id="md-chart-red"><rect y="50" width="100" height="50" /></clipPath>
                <linearGradient id="md-fill-blue" x1="0" x2="0" y1="0" y2="1"><stop stopColor="var(--md-blue)" stopOpacity=".24" /><stop offset="1" stopColor="var(--md-blue)" stopOpacity=".02" /></linearGradient>
                <linearGradient id="md-fill-red" x1="0" x2="0" y1="0" y2="1"><stop stopColor="var(--md-red)" stopOpacity=".02" /><stop offset="1" stopColor="var(--md-red)" stopOpacity=".24" /></linearGradient>
              </defs>
              {[0, 25, 50, 75, 100].map(y => <line key={y} x1="0" x2="100" y1={y} y2={y} className={y === 50 ? "md-zero-line" : "md-grid-line"} />)}
              {[0, 25, 50, 75, 100].map(x => <line key={x} x1={x} x2={x} y1="0" y2="100" className="md-grid-line" />)}
              <polygon points={`0,50 ${line} 100,50`} clipPath="url(#md-chart-blue)" className="md-area-blue" /><polygon points={`0,50 ${line} 100,50`} clipPath="url(#md-chart-red)" className="md-area-red" />
              <polyline points={line} clipPath="url(#md-chart-blue)" className="md-line-blue" /><polyline points={line} clipPath="url(#md-chart-red)" className="md-line-red" />
            </svg>
            {chart.points.map(point => <i key={point.timestamp} className={`md-frame-dot ${point.goldDiff === 0 ? "" : side(point.goldDiff > 0 ? 100 : 200)}${inspecting && point === active ? " is-active" : ""}`} style={{left: `${point.x}%`, top: `${point.y}%`}} aria-hidden />)}
            {inspecting && <i className="md-chart-cursor" aria-hidden />}
            <HoverCard.Root open={inspecting}>
              <HoverCard.Trigger asChild><span className="md-chart-anchor" style={{left: `${active.x}%`, top: `${active.y}%`}} aria-hidden /></HoverCard.Trigger>
              <HoverCard.Portal><HoverCard.Content className="md-timeline-popover" side="top" sideOffset={14} collisionPadding={16} onPointerEnter={keepOpen} onPointerLeave={hide} onEscapeKeyDown={close}>
                <div role="tooltip" id="md-minute-tooltip"><strong>{lead(active.goldDiff, active.timestamp)}</strong>
                  <div className="md-minute-events">{minuteEvents.length ? minuteEvents.map((event, i) => <div className={`md-minute-event ${side(event.teamId)}`} key={i}><time>{time(event.timestamp)}</time><EventContent event={event} name={name} /></div>) : <small>No events in this minute.</small>}</div>
                </div>
              </HoverCard.Content></HoverCard.Portal>
            </HoverCard.Root>
          </div>
        </div>
        <div className="md-event-lanes" aria-label="Team event lanes">{lanes.map(lane => <div className={`md-event-lane ${side(lane.teamId)}`} key={lane.teamId} aria-label={`${lane.teamId === 100 ? "Blue" : "Red"} team events`}>
          <span className="md-lane-label">{lane.teamId === 100 ? "Blue" : "Red"}</span><div className="md-lane-track">{lane.marks.map((mark, i) => {
            const first = mark.events[0], objective = first.kind === "objective";
            return <button key={i} className={`md-lane-mark ${objective ? "is-objective" : "is-kill"}${mark.events.some(personalKill) ? " is-personal" : ""}`}
              style={{left: `${(mark.timestamp - chart.start) / chart.duration * 100}%`, "--dot-size": `${Math.min(14, 6 + (mark.events.length - 1) * 3)}px`} as CSSProperties}
              aria-label={mark.events.map(event => `${time(event.timestamp)} · ${eventLabel(event, name)}`).join("; ")}
              onPointerEnter={event => inspectEvents(mark.events, event.currentTarget)} onPointerLeave={hide} onFocus={event => inspectEvents(mark.events, event.currentTarget)} onBlur={hide} onClick={event => inspectEvents(mark.events, event.currentTarget)}>
              {objective ? <ObjectiveIcon label={first.objective} side={lane.teamId} /> : <i aria-hidden />}
            </button>;
          })}</div>
        </div>)}</div>
        <div className="md-chart-times">{[0, .25, .5, .75, 1].map(fraction => <span key={fraction}>{time(chart.start + chart.duration * fraction)}</span>)}</div>
        <label className="md-chart-select">Inspect minute<input type="range" min={0} max={chart.points.length - 1} value={selected ?? chart.points.length - 1} onFocus={showFrame} onBlur={hide} onChange={event => {setSelected(Number(event.target.value)); showFrame();}} aria-label="Inspect recorded minute" aria-describedby={inspecting ? "md-minute-tooltip" : undefined} aria-valuetext={lead(active.goldDiff, active.timestamp)} /></label>
        <p className="md-chart-note">Lanes: objectives and kills · Larger dots group kills within 10 seconds · Rings involve you{participant != null ? ` · Showing kills involving ${name(players.find(player => player.participantId === participant)!.championId)}` : ""}</p>
      </div>
      {positionedKills.length > 0 && <KillMap kills={mapKills} total={positionedKills.length} players={players} selected={participant} onSelect={selectPlayer} inspect={inspectEvents} close={hide} name={name} />}
    </div>
    <div className="md-event-heading"><h3>Match events <small>{filtered.length}</small></h3><div className="md-event-filters" role="group" aria-label="Filter match events">{(["Key moments", "All", "Objectives", "Kills"] as const).map(label => <button key={label} aria-pressed={filter === label} onClick={() => {setFilter(label); setExpanded(false); close();}}>{label}</button>)}</div></div>
    <div className="md-event-list" aria-label="Match timeline">{filtered.length ? (expanded ? filtered : filtered.slice(0, 12)).map((event, i) => <button key={i} className={`${side(event.teamId)}${personalKill(event) ? " is-personal" : ""}`} onClick={e => inspectEvents([event], e.currentTarget)} onFocus={e => inspectEvents([event], e.currentTarget)} onBlur={hide} aria-label={`${time(event.timestamp)} · ${eventLabel(event, name)}`}>
      <time>{time(event.timestamp)}</time><EventContent event={event} name={name} />
    </button>) : <p>No events match this filter.</p>}</div>
    {filtered.length > 12 && <div className="md-event-expand"><span>Showing {expanded ? filtered.length : 12} of {filtered.length}</span><button aria-expanded={expanded} onClick={() => {setExpanded(!expanded); close();}}>{expanded ? "Show fewer" : "Show all"}</button></div>}
    <HoverCard.Root open={eventInspection != null}>
      <HoverCard.Trigger asChild><span className="md-event-anchor" style={{left: eventInspection?.x ?? 0, top: eventInspection?.y ?? 0}} aria-hidden /></HoverCard.Trigger>
      <HoverCard.Portal><HoverCard.Content className="md-timeline-popover" side="top" sideOffset={10} collisionPadding={16} onPointerEnter={keepOpen} onPointerLeave={hide} onEscapeKeyDown={close}>
        <div role="tooltip" id="md-event-tooltip">{eventInspection?.events.map((event, i) => <div className={`md-minute-event ${side(event.teamId)}`} key={i}><time>{time(event.timestamp)}</time><EventContent event={event} name={name} /></div>)}</div>
      </HoverCard.Content></HoverCard.Portal>
    </HoverCard.Root>
  </section>;
}
