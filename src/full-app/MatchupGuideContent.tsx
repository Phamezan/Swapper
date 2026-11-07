import { AbilityHover } from "./AbilityHover";
import { GuideItem } from "./GuideItem";
import { GuideSourceButton } from "./GuideSourceButton";
import { RankCrest } from "../runes/RankCrest";
import { Notice } from "./ui/States";
import { Tooltip } from "./ui/Tooltip";
import { GuideTipText, useGuideReferences } from "./GuideTipText";
import { parseGuideText } from "./guideText";
import type { GuideSource, GuideTip, GuideSectionId, AuthoredGuide } from "./matchupGuide";

export type GuideAbilities = Record<number, { key: string; name: string; icon: string | null }[]>;

const AUTHOR_COLORS = ["var(--ui-brand)", "var(--ui-win)", "var(--ui-loss)", "var(--ui-warn)"];
const SOURCE_KINDS = { mobafire: "MOBAFire guide", sheet: "Matchup sheet", youtube: "YouTube guide", web: "Web guide" };
const guideAuthors = (sources: GuideSource[]) => sources.filter((source, i) => sources.findIndex(other => other.author === source.author) === i);
const authorColor = (index: number) => AUTHOR_COLORS[index] ?? `hsl(${index * 137.508 % 360} 65% 70%)`;

export function GuideAuthorCredit({ source, color }: { source: GuideSource; color?: string }) {
  const main = source.rankClaim.match(/\b[\p{L}\p{N}]+(?: [\p{L}\p{N}]+)? (?:main|OTP)\b/u)?.[0];
  const rank = source.rank ? `${source.peak ? "Peak " : ""}${source.rank[0].toUpperCase() + source.rank.slice(1)}` : null;
  return <div className="mg-author-credit">
    {source.rank && <RankCrest className="mg-author-crest" tier={source.rank} size={28} mode="desktop" />}
    <div className="mg-author-details"><div className="mg-author-heading">
      {color && <span className="mg-author-dot" style={{ backgroundColor: color }} aria-hidden />}
      <strong>{source.author}</strong>
    </div><Tooltip label={source.rankClaim}><small aria-label={source.rankClaim}>{[rank, source.rank ? main : source.rankClaim, SOURCE_KINDS[source.kind]].filter(Boolean).join(" · ")}</small></Tooltip></div>
    <GuideSourceButton className="mg-original-link" url={source.url} label={`Read original by ${source.author}`}><span className="mg-original-label">Read original </span><span aria-hidden>↗</span></GuideSourceButton>
  </div>;
}

export function GuideAuthorCredits({ sources }: { sources: GuideSource[] }) {
  const authors = guideAuthors(sources);
  if (!authors.length) return null;
  return <div className={`mg-author-credits${authors.length > 1 ? " is-multi-author" : ""}`} aria-label="Guide authors">
    {authors.map((source, i) => <GuideAuthorCredit key={source.id} source={source} color={authors.length > 1 ? authorColor(i) : undefined} />)}
  </div>;
}

export function GuideTips({ tips, sources, abilities = {} }: { tips: GuideTip[]; sources: GuideSource[]; abilities?: GuideAbilities }) {
  const authors = guideAuthors(sources);
  return <ol className="mg-tips">{tips.map((tip, i) => <li key={i}>
    <span className="mg-tip-number" aria-hidden>{String(i + 1).padStart(2, "0")}</span>
    <GuideTipContent tip={tip} sources={sources} authors={authors} abilities={abilities} />
  </li>)}</ol>;
}

function GuideTipContent({ tip, sources, authors, abilities }: { tip: GuideTip; sources: GuideSource[]; authors: GuideSource[]; abilities: GuideAbilities }) {
  const references = useGuideReferences(abilities, tip.abilityKeys ?? [], tip.itemIds ?? []);
  const parts = parseGuideText(tip.text, references);
  const heading = tip.heading ? parseGuideText(tip.heading, references) : [];
  const inline = [...parts, ...heading].filter(part => !("text" in part));
  const abilityKeys = tip.abilityKeys?.filter(ref => !inline.some(part => "champion" in part && part.champion === ref.champion && part.key === ref.key)) ?? [];
  const itemIds = tip.itemIds?.filter(id => !inline.some(part => "kind" in part && part.kind === "item" && part.id === id)) ?? [];
  const credited = authors.length > 1 ? guideAuthors(tip.sourceIds?.flatMap(id => sources.filter(source => source.id === id)) ?? []) : [];
  const credits = credited.length > 0 && <span className="mg-tip-credits">{credited.map(source =>
    <Tooltip key={source.id} label={source.author}><span className="mg-author-dot" role="img" aria-label={source.author} style={{ backgroundColor: authorColor(authors.findIndex(author => author.author === source.author)) }} /></Tooltip>)}</span>;
  return <div className="mg-tip-content">
      {tip.heading && <h3 className="mg-tip-heading"><GuideTipText parts={heading} abilities={abilities} />{credits}</h3>}
      <p><GuideTipText parts={parts} abilities={abilities} />{!tip.heading && credits}</p>
      {tip.patch && <div className="mg-tip-meta"><span>Written {tip.patch}</span></div>}
      {(abilityKeys.length || itemIds.length) ? <div className="mg-references">
        {abilityKeys.map(ref => {
          const ability = abilities[ref.champion]?.find(ability => ability.key === ref.key);
          return <AbilityHover key={`${ref.champion}-${ref.key}`} championId={ref.champion} abilityKey={ref.key} icon={ability?.icon}>
            {ability?.icon && <img src={ability.icon} alt="" />}<span>{ref.key}{ability?.name ? ` · ${ability.name}` : ""}</span>
          </AbilityHover>;
        })}
        {itemIds.map(id => <GuideItem key={id} id={id} />)}
      </div> : null}
      {tip.outdated && <Notice tone="warn">May be outdated: {tip.outdated === true ? "a champion changed after this tip's patch." : tip.outdated}</Notice>}
    </div>;
}

export function GuideSectionContent({ guide, section, abilities, showAuthors = true }: { guide: AuthoredGuide; section: GuideSectionId; abilities: GuideAbilities; showAuthors?: boolean }) {
  const content = guide.sections[section];
  const tips = [...content.tips, ...(guide.enemyTips?.[section] ?? [])];
  const visibleTips = section === "overview" && !showAuthors
    ? tips.filter(tip => tip.text.trim() !== guide.favoredWhen?.trim()) : tips;
  return <div className="mg-section-content">
    {showAuthors && <GuideAuthorCredits sources={guide.sources} />}
    <GuideTips tips={visibleTips} sources={guide.sources} abilities={abilities} />
    {content.alternatives && <div className="mg-alternatives">{content.alternatives.map(style => {
      return <section className="ui-well" key={style.title}>
      <h3>{style.title}</h3>
      <GuideTips tips={style.tips} sources={guide.sources} abilities={abilities} />
    </section>; })}</div>}
    {content.timeline && content.timeline.length > 0 && <table className="mg-written-timeline">
      <caption>Lane timeline</caption>
      <thead><tr><th scope="col">Timing</th><th scope="col">You</th><th scope="col">Opponent</th></tr></thead>
      <tbody>{content.timeline.map((row, i) => <tr key={i}><th scope="row">{row.at}</th><td>{row.champion}</td><td>{row.enemy}</td></tr>)}</tbody>
    </table>}
  </div>;
}
