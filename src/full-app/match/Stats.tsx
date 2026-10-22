import { Fragment, useMemo, useState } from "react";
import { ChampionIcon } from "../../runes/ChampionIcon";
import { buildStatGroups, filterStatGroups, type MatchPlayer, type StatRow, type StatValue } from "../MatchDetailModel";
import { SearchIcon } from "../ui/icons";
import { fmtCount } from "../ui/format";

function format(value: StatValue, kind: StatRow["format"]) {
  if (value == null) return "–";
  if (typeof value === "boolean") return value ? "Yes" : "No";
  if (kind === "ratio") return value.toFixed(2);
  if (kind === "time") return `${Math.floor(Math.round(value) / 60)}:${String(Math.round(value) % 60).padStart(2, "0")}`;
  return Number.isInteger(value) ? fmtCount(value) : value.toLocaleString("en-US", { maximumFractionDigits: 2 });
}

export function Stats({ players, name }: { players: MatchPlayer[]; name: (id: number) => string }) {
  const [search, setSearch] = useState("");
  const [collapsed, setCollapsed] = useState(new Set<string>());
  const columns = useMemo(() => [...players].sort((a, b) => a.teamId - b.teamId || a.participantId - b.participantId), [players]);
  const groups = useMemo(() => buildStatGroups(columns), [columns]);
  const filtered = useMemo(() => filterStatGroups(groups, search), [groups, search]);
  const count = filtered.reduce((n, group) => n + group.rows.length, 0);
  const toggle = (label: string) => setCollapsed(old => { const next = new Set(old); if (next.has(label)) next.delete(label); else next.add(label); return next; });
  return <section className="md-stats ui-card" aria-label="All player stats">
    <header className="md-view-heading">
      <div><h2>Player statistics</h2><p>Row leaders are marked · Higher totals, fewer deaths · “–” means unavailable</p></div>
      <label className="md-stat-search"><SearchIcon size={18} /><input type="search" placeholder="Search a stat" aria-label="Search a stat" value={search} onChange={e => setSearch(e.target.value)} /></label>
    </header>
    <div className="md-stats-meta" aria-live="polite"><span>{count} {count === 1 ? "stat" : "stats"}{search.trim() ? ` matching “${search.trim()}”` : ` across ${groups.length} groups`}</span><span className="md-own-key">Your column</span></div>
    <div className="md-stats-scroll" tabIndex={0} role="region" aria-label="Player statistics table">
      <table className="md-stats-table">
        <caption className="md-sr-only">Stats grouped by team. Tied leaders share a highlight. Uniform or incomplete rows have no leader.</caption>
        <colgroup><col className="md-label-col" />{columns.map(p => <col key={p.participantId} />)}</colgroup>
        <thead>
          <tr className="md-stats-sides"><th scope="col">Match statistics</th>{[100, 200].map(id => {
            const size = columns.filter(p => p.teamId === id).length;
            return size ? <th scope="colgroup" colSpan={size} key={id} className={id === 100 ? "is-blue" : "is-red"}>{id === 100 ? "Blue" : "Red"} team</th> : null;
          })}</tr>
          <tr className="md-stats-portraits"><th scope="col">{columns.length} players <small>Compare the whole match</small></th>{columns.map((p, i) => <th scope="col" key={p.participantId} className={`${p.me ? "is-me" : ""} ${i > 0 && p.teamId !== columns[i - 1].teamId ? "md-side-start" : ""}`} title={`${name(p.championId)}${p.me ? " · You" : ""}`}>
            <ChampionIcon id={p.championId} name={name(p.championId)} mode="desktop" /><span>{name(p.championId)}</span><small>{p.me ? "You" : p.role === "adc" ? "Bot" : p.role.charAt(0).toUpperCase() + p.role.slice(1)}</small>
          </th>)}</tr>
        </thead>
        {filtered.map(group => {
          const open = !!search.trim() || !collapsed.has(group.label);
          const id = `md-group-${group.label.replace(/\W+/g, "-")}`;
          return <Fragment key={group.label}><tbody className="md-stats-group"><tr><th colSpan={columns.length + 1}>
            <button onClick={() => toggle(group.label)} aria-expanded={open} aria-controls={id} disabled={!!search.trim()}><span aria-hidden>{open ? "−" : "+"}</span>{group.label}<small>{group.rows.length}</small></button>
          </th></tr></tbody><tbody id={id} hidden={!open}>{group.rows.map(row => <tr key={row.key}>
            <th scope="row" title={row.key}>{row.label}</th>{row.values.map((v, i) => <td key={columns[i].participantId} className={`${columns[i].me ? "is-me" : ""} ${row.leaders.includes(i) ? "is-best" : ""} ${i > 0 && columns[i].teamId !== columns[i - 1].teamId ? "md-side-start" : ""}`}>
              {format(v, row.format)}{row.leaders.includes(i) && <span className="md-sr-only"> (row leader)</span>}
            </td>)}
          </tr>)}</tbody></Fragment>;
        })}
      </table>
      {!filtered.length && <div className="md-stats-empty">No stats match “{search.trim()}”.<button className="fa-text-button" onClick={() => setSearch("")}>Clear search</button></div>}
    </div>
  </section>;
}
