import { useMemo, type CSSProperties } from "react";
import { ChampionIcon } from "../../runes/ChampionIcon";
import { recapRows, timelineChart, type MatchPlayer, type Timeline } from "../MatchDetailModel";
import type { MatchDetailTeam } from "../types";
import { ObjectiveIcon } from "../ObjectiveIcon";
import { fmtCompact } from "../ui/format";
import { time } from "./GraphEvents";

const ROLES = ["top", "jungle", "mid", "adc", "support"];
const byRole = (a: MatchPlayer, b: MatchPlayer) => (ROLES.indexOf(a.role) + 1 || 9) - (ROLES.indexOf(b.role) + 1 || 9);
const MARKED = new Set(["Dragon", "Elder", "Baron", "Herald"]);
const OBJECTIVE_ICON: Record<string, string> = { Dragon: "Dragons", Elder: "Elders", Baron: "Barons", Herald: "Heralds" };

function Bans({ ids, side, name }: { ids?: number[]; side: string; name: (id: number) => string }) {
  return <span role="cell" className={`md-recap-bans ${side}`}>{(ids ?? []).map(id => <ChampionIcon key={id} id={id} name={name(id)} mode="desktop" />)}</span>;
}

/** Broadcast-style post-game recap: result, stat ladder, damage dealt and the gold swing. */
export function Recap({ players, teams, timeline, duration, name }: {
  players: MatchPlayer[]; teams: MatchDetailTeam[]; timeline?: Timeline | null; duration: number; name: (id: number) => string;
}) {
  const rows = useMemo(() => recapRows(players, teams, timeline?.events), [players, teams, timeline]);
  const chart = useMemo(() => timeline ? timelineChart(timeline) : null, [timeline]);
  const sides = [100, 200].map(id => ({ id, team: teams.find(t => t.teamId === id), players: players.filter(p => p.teamId === id).sort(byRole) }));
  const maxDamage = Math.max(1, ...players.map(p => p.damage ?? 0));
  const ownTeam = players.find(p => p.me)?.teamId;
  const line = chart?.points.map(p => `${p.x},${p.y}`).join(" ");
  const result = (id: number) => { const win = sides.find(s => s.id === id)?.team?.win; return win == null ? "" : win ? "Win" : "Loss"; };
  const hasBans = sides.some(s => s.team?.bans?.length);
  return <section className="md-comparison md-recap ui-card" aria-label="Match recap">
    <header className="md-recap-head">
      {[100, 0, 200].map(id => id ? <strong key={id} className={`md-recap-result ${id === 100 ? "is-blue" : "is-red"}`}>
        {id === 100 ? "Blue" : "Red"} {result(id)}{ownTeam === id && <small>Your team</small>}
      </strong> : <div key="time" className="md-recap-time"><small>Game time</small><b>{time(duration * 1000)}</b></div>)}
    </header>
    <div className="md-recap-body">
      <div className="md-recap-stats" role="table" aria-label="Team stats">
        {rows.map(row => <div className="md-recap-row" role="row" key={row.label}>
          <b role="cell" className={`is-blue${row.lead === 100 ? " is-leading" : ""}`}>{row.values[0] ?? "–"}</b>
          <span role="rowheader">{row.objective && <span className="md-recap-icons" aria-hidden><ObjectiveIcon label={row.objective} side={100} /><ObjectiveIcon label={row.objective} side={200} /></span>}{row.label}</span>
          <b role="cell" className={`is-red${row.lead === 200 ? " is-leading" : ""}`}>{row.values[1] ?? "–"}</b>
        </div>)}
        {hasBans && <div className="md-recap-row is-bans" role="row">
          <Bans ids={sides[0].team?.bans} side="is-blue" name={name} /><span role="rowheader">Bans</span><Bans ids={sides[1].team?.bans} side="is-red" name={name} />
        </div>}
      </div>
      <div className="md-recap-side">
        <div className="md-recap-damage" role="group" aria-label="Total damage dealt to champions">
          <h3>Total damage dealt</h3>
          {ROLES.map((_, i) => <div className="md-recap-duel" key={i}>
            {sides.map(side => {
              const p = side.players[i];
              if (!p) return <span key={side.id} />;
              return <div key={side.id} className={`md-recap-dealer ${side.id === 100 ? "is-blue" : "is-red"}${p.me ? " is-me" : ""}`}
                aria-label={`${p.riotId || name(p.championId)}: ${p.damage == null ? "unavailable" : fmtCompact(p.damage)} damage`}>
                <ChampionIcon id={p.championId} name={name(p.championId)} mode="desktop" />
                <span><small>{(p.riotId || name(p.championId)).split("#")[0]}</small><b>{p.damage == null ? "–" : fmtCompact(p.damage)}</b>
                  <i style={{ "--share": `${(p.damage ?? 0) / maxDamage * 100}%` } as CSSProperties} /></span>
              </div>;
            })}
          </div>)}
        </div>
        {chart && line && <div className="md-recap-gold" role="img" aria-label={`Gold difference over the game, peaking at ${fmtCompact(chart.limit)}`}>
          <h3>Gold difference</h3>
          <div className="md-recap-chart">
            <span className="md-recap-axis is-top">+{fmtCompact(chart.limit)}</span><span className="md-recap-axis is-bottom">−{fmtCompact(chart.limit)}</span>
            <svg viewBox="0 0 100 100" preserveAspectRatio="none" aria-hidden>
              <defs><clipPath id="md-recap-blue"><rect width="100" height="50" /></clipPath><clipPath id="md-recap-red"><rect y="50" width="100" height="50" /></clipPath></defs>
              <line x1="0" x2="100" y1="50" y2="50" className="md-zero-line" />
              <polygon points={`0,50 ${line} 100,50`} clipPath="url(#md-recap-blue)" className="md-recap-area is-blue" />
              <polygon points={`0,50 ${line} 100,50`} clipPath="url(#md-recap-red)" className="md-recap-area is-red" />
            </svg>
            <div className="md-recap-markers" aria-hidden>{chart.events.filter(e => MARKED.has(e.label)).map((e, i) =>
              <span key={i} style={{ left: `${e.x}%` }} title={`${e.teamId === 100 ? "Blue" : "Red"} ${e.label.toLowerCase()} · ${time(e.timestamp)}`}><ObjectiveIcon label={OBJECTIVE_ICON[e.label]} side={e.teamId} /></span>)}</div>
          </div>
        </div>}
      </div>
    </div>
  </section>;
}
