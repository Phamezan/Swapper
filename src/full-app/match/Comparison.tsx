import { useMemo, type CSSProperties } from "react";
import { comparisonRows, type MatchPlayer } from "../MatchDetailModel";
import type { MatchDetailTeam } from "../types";
import { fmtCompact, fmtCount } from "../ui/format";

export function Comparison({ players, teams }: { players: MatchPlayer[]; teams: MatchDetailTeam[] }) {
  const rows = useMemo(() => comparisonRows(players, teams), [players, teams]);
  return <section className="md-comparison ui-card" aria-label="Team comparison">
    <header><h2>Team comparison</h2><div className="md-team-key"><span className="is-blue">Blue team{players.some(p => p.me && p.teamId === 100) ? " · You" : ""}</span><span className="is-red">Red team{players.some(p => p.me && p.teamId === 200) ? " · You" : ""}</span></div></header>
    <div className="md-comparison-rows">{rows.map(({label, values: [blue, red]}) => {
      const known = blue != null && red != null, total = (blue ?? 0) + (red ?? 0);
      const value = (v: number | null) => v == null ? "–" : label === "Gold earned" || label === "Damage to champions" ? fmtCompact(v) : fmtCount(v);
      return <div className="md-compare-row" key={label}>
        <span>{label}</span><b className={`is-blue ${known && blue > red ? "is-leading" : ""}`}>{value(blue)}</b>
        <div className={`md-split-bar${known ? "" : " is-unknown"}`} role="img" aria-label={`${label}: blue ${blue == null ? "unavailable" : fmtCount(blue)}, red ${red == null ? "unavailable" : fmtCount(red)}`}
          style={{ "--blue-share": `${known && total ? blue / total * 100 : 50}%` } as CSSProperties}><i /><i /></div>
        <b className={`is-red ${known && red > blue ? "is-leading" : ""}`}>{value(red)}</b>
      </div>;
    })}</div>
  </section>;
}
