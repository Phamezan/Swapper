import type { CSSProperties } from "react";
import type { MatchFact } from "../MatchDetailModel";
import { ObjectiveIcon } from "../ObjectiveIcon";
import { Icon } from "../ui/icons/Icon";
import { MatchupsIcon } from "../ui/icons";
import { fmtCompact, fmtCount } from "../ui/format";

export function Facts({ facts }: { facts: MatchFact[] }) {
  return <div className="md-match-facts" aria-label="The match at a glance">{facts.map(fact => {
    const side = fact.teamId === 100 ? "blue" : "red", enemy = side === "blue" ? "red" : "blue";
    const values = "values" in fact ? fact.values : null;
    const tone = values ? values[0] > values[1] ? "ahead" : values[0] < values[1] ? "behind" : "even" : "even";
    return <article key={fact.kind} className={`md-fact md-fact-${fact.kind} is-${tone}`} style={{ "--fact-side": `var(--md-${side})`, "--fact-enemy": `var(--md-${enemy})` } as CSSProperties}>
      <h3><span className="md-fact-icon" aria-hidden>{fact.kind === "towers" ? <ObjectiveIcon label="Towers" side={fact.teamId} /> : fact.kind === "damage" ? <MatchupsIcon size={18} /> : fact.kind === "gold" ? <Icon size={18}><ellipse cx="12" cy="7" rx="7" ry="3" /><path d="M5 7v5c0 1.7 3.1 3 7 3s7-1.3 7-3V7M5 12v5c0 1.7 3.1 3 7 3s7-1.3 7-3v-5" /></Icon> : <Icon size={18}><path d="M3 12s3-6 9-6 9 6 9 6-3 6-9 6-9-6-9-6Z" /><circle cx="12" cy="12" r="2.5" /></Icon>}</span>{fact.label}</h3>
      {fact.kind === "towers" ? <>
        <div className="md-tower-reading" aria-label={`Your team ${values![0]} towers, enemy ${values![1]} towers`}>
          <div><strong className={`is-${side}`}>{values![0]}</strong><span className={`is-${side}`}>Your team</span></div><span aria-hidden>:</span>
          <div><strong className={`is-${enemy}`}>{values![1]}</strong><span className={`is-${enemy}`}>Enemy</span></div>
        </div>
      </> : <div className="md-fact-reading"><div><strong>{fact.value}</strong><small>{fact.detail}</small></div>
        {fact.kind === "damage" && <svg className="md-share-ring" viewBox="0 0 48 48" role="img" aria-label={`You dealt ${fmtCount(fact.damage)} of your team's ${fmtCount(fact.teamDamage)} damage to champions (${fact.value})`}>
          <circle cx="24" cy="24" r="19" /><circle cx="24" cy="24" r="19" pathLength="100" strokeDasharray={`${fact.share} 100`} /><circle className="md-share-center" cx="24" cy="24" r="4" />
        </svg>}
      </div>}
      {values ? <div className="md-fact-comparison">
        <div className={`md-fact-bar${values[0] + values[1] === 0 ? " is-empty" : ""}`} aria-hidden style={{ "--own-share": `${values[0] + values[1] > 0 ? values[0] / (values[0] + values[1]) * 100 : 0}%` } as CSSProperties}><i /><i /></div>
        {fact.kind === "gold" && <div className="md-fact-key"><span className={`is-${side}`}>Your team · {fmtCompact(values[0])}</span><span className={`is-${enemy}`}>Enemy · {fmtCompact(values[1])}</span></div>}
      </div> : fact.kind === "damage" && <div className="md-fact-key"><span className={`is-${side}`}>You · {fmtCompact(fact.damage)}</span><span>Team · {fmtCompact(fact.teamDamage)}</span></div>}
    </article>;
  })}</div>;
}
