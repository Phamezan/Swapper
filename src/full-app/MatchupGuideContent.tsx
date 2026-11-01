import { AbilityHover } from "./AbilityHover";
import { GuideItem } from "./GuideItem";
import { GuideSourceButton } from "./GuideSourceButton";
import { Notice } from "./ui/States";
import { GuideTipText } from "./GuideTipText";
import type { GuideSource, GuideTip, GuideSectionId, AuthoredGuide } from "./matchupGuide";

export type GuideAbilities = Record<number, { key: string; name: string; icon: string | null }[]>;

export function GuideSourceChip({ source }: { source: GuideSource }) {
  return <GuideSourceButton className="mg-source" url={source.url}>
    <span className="mg-avatar" aria-hidden>{source.author.slice(0, 1)}</span>
    <span><strong>{source.author}</strong><small>{source.rankClaim}</small></span><span aria-hidden>↗</span>
  </GuideSourceButton>;
}

export function GuideTips({ tips, sources, abilities = {} }: { tips: GuideTip[]; sources: GuideSource[]; abilities?: GuideAbilities }) {
  return <ol className="mg-tips">{tips.map((tip, i) => <li key={i}>
    <div className="mg-tip-authors">{tip.sourceIds?.map(id => sources.find(source => source.id === id)).filter((source): source is GuideSource => !!source).map(source =>
      <GuideSourceButton key={source.id} className="mg-tip-author" url={source.url} label={source.author}>
        <span className="mg-tip-author-name">{source.author}</span><span className="mg-tip-author-initials" aria-hidden>{source.author.trim().split(/\s+/).slice(0, 2).map(part => part.slice(0, 1)).join("").toUpperCase()}</span>
      </GuideSourceButton>)}</div>
    <span className="mg-tip-number" aria-hidden>{String(i + 1).padStart(2, "0")}</span>
    <div className="mg-tip-content">{tip.heading && <h3 className="mg-tip-heading">{tip.heading}</h3>}<p><GuideTipText text={tip.text} abilities={abilities} /></p>
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
    <GuideTips tips={[...content.tips, ...(guide.enemyTips?.[section] ?? [])]} sources={guide.sources} abilities={abilities} />
    {content.alternatives && <div className="mg-alternatives">{content.alternatives.map(style => {
      const source = guide.sources.find(source => source.id === style.sourceId);
      return <section className="ui-well" key={style.title}>
      <h3>{style.title}</h3>
      {source && <GuideSourceChip source={source} />}
      <GuideTips tips={style.tips} sources={guide.sources} abilities={abilities} />
    </section>; })}</div>}
  </div>;
}
