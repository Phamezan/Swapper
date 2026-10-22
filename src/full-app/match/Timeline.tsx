import { useMemo, useState, type CSSProperties } from "react";
import { timelineChart, type Timeline as TimelineData } from "../MatchDetailModel";
import { fmtCompact, fmtCount } from "../ui/format";

const time = (ms: number) => `${Math.floor(ms / 60000)}:${String(Math.floor(ms / 1000) % 60).padStart(2, "0")}`;
const lead = (value: number) => value === 0 ? "Teams even" : `${value > 0 ? "Blue" : "Red"} +${fmtCount(Math.abs(value))} gold`;

export function Timeline({ data }: { data: TimelineData }) {
  const chart = useMemo(() => timelineChart(data), [data]);
  const [selected, setSelected] = useState<number | null>(null);
  if (!chart) return null;
  const active = chart.points[selected ?? chart.points.length - 1];
  const line = chart.points.map(p => `${p.x},${p.y}`).join(" ");
  return <section className="md-timeline ui-card" aria-label="Gold difference over time">
    <header className="md-view-heading"><div><h2>Gold over time</h2><p>Blue team − red team · {chart.points.length} recorded frames · Select a minute to inspect</p></div><div className="md-chart-reading"><b>{lead(active.goldDiff)}</b><span>At {time(active.timestamp)}</span></div></header>
    <div className="md-chart-legend"><span className="is-blue">Blue advantage</span><span className="is-red">Red advantage</span><span>◆ Objective taken</span></div>
    <div className="md-chart-wrap">
      <div className="md-chart-axis"><span>+{fmtCompact(chart.limit)}</span><span>Even</span><span>−{fmtCompact(chart.limit)}</span></div>
      <div className="md-chart" style={{ "--cursor": `${active.x}%` } as CSSProperties}>
        <svg viewBox="0 0 100 100" preserveAspectRatio="none" aria-hidden>
          <defs><clipPath id="md-chart-blue"><rect width="100" height="50" /></clipPath><clipPath id="md-chart-red"><rect y="50" width="100" height="50" /></clipPath></defs>
          {[0, 25, 50, 75, 100].map(y => <line key={y} x1="0" x2="100" y1={y} y2={y} className={y === 50 ? "md-zero-line" : "md-grid-line"} />)}
          <polygon points={`0,50 ${line} 100,50`} clipPath="url(#md-chart-blue)" className="md-area-blue" /><polygon points={`0,50 ${line} 100,50`} clipPath="url(#md-chart-red)" className="md-area-red" />
          <polyline points={line} clipPath="url(#md-chart-blue)" className="md-line-blue" /><polyline points={line} clipPath="url(#md-chart-red)" className="md-line-red" />
        </svg>
        <i className="md-chart-cursor" aria-hidden />
        {chart.events.map((event, i) => <span className={`md-chart-event ${event.teamId === 100 ? "is-blue" : "is-red"}`} key={i} title={`${time(event.timestamp)} · ${event.teamId === 100 ? "Blue" : "Red"} team took ${event.label.toLowerCase()}`} style={{left: `${event.x}%`, top: `${event.y}%`}}>◆</span>)}
        <span className={`md-chart-point ${active.goldDiff >= 0 ? "is-blue" : "is-red"}`} style={{left: `${active.x}%`, top: `${active.y}%`}} aria-hidden />
      </div>
    </div>
    <div className="md-chart-times">{[0, .25, .5, .75, 1].map(f => <span key={f}>{time(chart.start + chart.duration * f)}</span>)}</div>
    <label className="md-chart-select">Recorded frame<input type="range" min={0} max={chart.points.length - 1} value={selected ?? chart.points.length - 1} onChange={e => setSelected(Number(e.target.value))} aria-label="Inspect recorded minute" aria-valuetext={`${time(active.timestamp)}: ${lead(active.goldDiff)}`} /></label>
    <p className="md-chart-note">The line joins recorded frames. Objective markers sit between the surrounding frames.</p>
    <div className="md-event-list" aria-label="Objective timeline">{chart.events.length ? chart.events.map((event, i) => <button key={i} className={event.teamId === 100 ? "is-blue" : "is-red"} onClick={() => {
      const index = chart.points.findIndex(p => p.timestamp >= event.timestamp); setSelected(index);
    }} title="Inspect the next recorded frame"><time>{time(event.timestamp)}</time><b>{event.label}</b><span>{event.teamId === 100 ? "Blue" : "Red"} team</span></button>) : <p>No objective events were returned.</p>}</div>
  </section>;
}
