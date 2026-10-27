import type { CSSProperties } from "react";
import type { ChampionOption } from "../runes/types";
import { useAbilitySheet } from "./AbilityHover";
import { AbilityIcon } from "./CooldownSheet";
import { GuideVisual } from "./GuideVisual";
import { perRank } from "./cooldownModel";
import { cooldownRows, cooldownTicks, windowAbility } from "./matchupVisuals";
import "./matchup-visuals.css";

export function TradeWindows({ champion, enemy }: { champion: ChampionOption; enemy: ChampionOption }) {
  const { sheet: yours } = useAbilitySheet(champion.id, true);
  const { sheet: theirs } = useAbilitySheet(enemy.id, true);
  const ownRows = cooldownRows(yours), enemyRows = cooldownRows(theirs);
  if (!ownRows.length || !enemyRows.length) return null;
  const rows = [...ownRows.map(row => ({ ...row, side: "you", champion: champion.name })), ...enemyRows.map(row => ({ ...row, side: "enemy", champion: enemy.name }))];
  const basics = rows.filter(row => row.key !== "R"), ultimates = rows.filter(row => row.key === "R");
  const maximum = (group: typeof rows) => Math.max(1, ...group.flatMap(row => row.ranks.map(rank => rank.seconds)));
  const basicMax = maximum(basics), ultimateMax = maximum(ultimates);
  const tradeWindow = windowAbility(enemyRows);
  const renderRow = (row: typeof rows[number], scale: number) => {
    const last = row.ranks[row.ranks.length - 1].seconds;
    return <div key={`${row.side}-${row.key}`} className={`mg-trade-row is-${row.side}${row.side === "enemy" && row.key === tradeWindow?.key ? " is-window" : ""}`} style={{ "--bar": `${row.seconds / scale * 100}%` } as CSSProperties}>
      <span className="mg-trade-ability" title={`${row.champion} · ${row.name}`} aria-label={`${row.champion} ${row.key}, ${row.name}`}><AbilityIcon path={row.iconPath} letter={row.key} /></span>
      <span className="mg-trade-track"><i className="mg-trade-fill" aria-hidden />
        {cooldownTicks(row.ranks, scale).map(tick => <span key={tick.ranks.join("-")} className="mg-trade-tick" role="img" tabIndex={0} style={{ left: `${tick.percent}%` }} aria-label={`${row.champion} ${row.key}: ${tick.ranks.map(rank => `Rank ${rank}: ${perRank([tick.seconds])}s${row.recharge ? " recharge" : ""}`).join("; ")}`}>
          <span className="mg-trade-tick-tooltip" aria-hidden>{tick.ranks.map(rank => <span key={rank}>Rank {rank}: {perRank([tick.seconds])}s{row.recharge ? " recharge" : ""}</span>)}</span>
        </span>)}
        <span className="mg-trade-value">{row.seconds === last ? `${perRank([row.seconds])}s` : `${perRank([row.seconds])} → ${perRank([last])}s`}{row.recharge && <small> recharge</small>}</span>
      </span>
    </div>;
  };
  return <GuideVisual title="Trade windows" preference="trades" header={<div className="mg-visual-legend"><span className="is-you">{champion.name}</span><span className="is-enemy">{enemy.name}</span></div>}>
    {!!basics.length && <div className="mg-trade-basics">
      <div className="mg-trade-axis" aria-hidden>{[0, basicMax / 2, basicMax].map(value => <span key={value}>{perRank([value]) || "0"}s</span>)}</div>
      {basics.map(row => renderRow(row, basicMax))}
    </div>}
    {!!ultimates.length && <div className="mg-trade-ultimates"><p>Ultimates · separate 0–{perRank([ultimateMax])}s scale</p><div className="mg-ultimate-rows">{ultimates.map(row => renderRow(row, ultimateMax))}</div></div>}
    <p className="mg-visual-caption"><span>Base cooldowns before ability haste</span>{tradeWindow && <span className="mg-window-label">Your window (rank 1): {enemy.name} {tradeWindow.key}</span>}</p>
    <ul className="mg-visual-text">{rows.map(row => <li key={`${row.side}-${row.key}`}>{row.champion} {row.key}, {row.name}: {row.ranks.map(rank => `rank ${rank.rank}: ${perRank([rank.seconds])} seconds`).join("; ")}{row.recharge ? " recharge" : " cooldown"}.{row.side === "enemy" && row.key === tradeWindow?.key ? " Longest enemy basic cooldown at rank 1: your trade window." : ""}</li>)}</ul>
  </GuideVisual>;
}
