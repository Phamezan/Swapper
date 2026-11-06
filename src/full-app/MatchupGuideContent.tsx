import { AbilityHover } from "./AbilityHover";
import { GuideItem } from "./GuideItem";
import { GuideSourceButton } from "./GuideSourceButton";
import { RankCrest } from "../runes/RankCrest";
import { Notice } from "./ui/States";
import { GuideTipText } from "./GuideTipText";
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
    </div><small title={source.rankClaim}>{[rank, source.rank ? main : source.rankClaim, SOURCE_KINDS[source.kind]].filter(Boolean).join(" · ")}</small></div>
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
    <div className="mg-tip-content">{authors.length > 1 && <span className="mg-tip-credits">{guideAuthors(tip.sourceIds?.flatMap(id => sources.filter(source => source.id === id)) ?? []).map(source =>
      <span key={source.id} className="mg-author-dot" role="img" aria-label={source.author} title={source.author} style={{ backgroundColor: authorColor(authors.findIndex(author => author.author === source.author)) }} />)}</span>}
      {tip.heading && <h3 className="mg-tip-heading">{tip.heading}</h3>}<p><GuideTipText text={tip.text} abilities={abilities} abilityKeys={tip.abilityKeys} /></p>
      {tip.patch && <div className="mg-tip-meta"><span>Written {tip.patch}</span></div>}
      {(tip.abilityKeys?.length || tip.itemIds?.length) ? <div className="mg-references">
        {tip.abilityKeys?.map(ref => {
          const ability = abilities[ref.champion]?.find(ability => ability.key === ref.key);
          return <AbilityHover key={`${ref.champion}-${ref.key}`} championId={ref.champion} abilityKey={ref.key} icon={ability?.icon}>
            {ability?.icon && <img src={ability.icon} alt="" />}<span>{ref.key}{ability?.name ? ` · ${ability.name}` : ""}</span>
          </AbilityHover>;
        })}
        {tip.itemIds?.map(id => <GuideItem key={id} id={id} />)}
      </div> : null}
      {tip.outdated && <Notice tone="warn">May be outdated: {tip.outdated === true ? "a champion changed after this tip's patch." : tip.outdated}</Notice>}
    </div>
  </li>)}</ol>;
}

export function GuideSectionContent({ guide, section, abilities }: { guide: AuthoredGuide; section: GuideSectionId; abilities: GuideAbilities }) {
  const content = guide.sections[section];
  return <div className="mg-section-content">
    <GuideAuthorCredits sources={guide.sources} />
    <GuideTips tips={[...content.tips, ...(guide.enemyTips?.[section] ?? [])]} sources={guide.sources} abilities={abilities} />
    {content.alternatives && <div className="mg-alternatives">{content.alternatives.map(style => {
      return <section className="ui-well" key={style.title}>
      <h3>{style.title}</h3>
      <GuideTips tips={style.tips} sources={guide.sources} abilities={abilities} />
    </section>; })}</div>}
  </div>;
}
