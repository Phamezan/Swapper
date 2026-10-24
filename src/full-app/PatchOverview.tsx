import { useMemo } from "react";
import { ChampionIcon } from "../runes/ChampionIcon";
import { ItemIcon } from "../runes/ItemIcon";
import { patchOverview, type OverviewChampion, type OverviewLink } from "./patchOverviewModel";
import type { PatchSection } from "./PatchNotesModel";
import "./patch-overview.css";

/** Scrolls a card or section into the reading zone (~27% down), as the rail does. */
function jump(anchor: string) {
  const target = document.getElementById(anchor);
  const scroller = target?.closest<HTMLElement>(".dashboard-main");
  if (!target || !scroller) return;
  if (target instanceof HTMLDetailsElement) target.open = true;
  target.focus({ preventScroll: true });
  const top = scroller.scrollTop + target.getBoundingClientRect().top - scroller.getBoundingClientRect().top - scroller.clientHeight * .27;
  scroller.scrollTo({ top: Math.max(0, Math.min(scroller.scrollHeight - scroller.clientHeight, top)),
    behavior: matchMedia("(prefers-reduced-motion: reduce)").matches ? "instant" : "smooth" });
}

const GROUPS = [
  { key: "nerfs", title: "Nerfs", mark: "▼", tone: "is-nerf" },
  { key: "buffs", title: "Buffs", mark: "▲", tone: "is-buff" },
  { key: "adjusted", title: "Adjusted", mark: "↻", tone: "is-adjust" },
] as const;

function Champion({ champion }: { champion: OverviewChampion }) {
  return <li><button type="button" className="pov-champion" onClick={() => jump(champion.anchor)} aria-label={`${champion.name}: jump to changes`}>
    <span className="pov-portrait">{champion.championId != null
      ? <ChampionIcon id={champion.championId} name={champion.name} mode="desktop" />
      : <span className="pov-initial" aria-hidden>{champion.name.slice(0, 1)}</span>}</span>
    <span className="pov-name">{champion.name}</span>
  </button></li>;
}

function Items({ items }: { items: OverviewLink[] }) {
  if (!items.length) return null;
  return <div className="pov-links"><h3>Items</h3><ul className="pov-items">{items.map(item => <li key={item.anchor}>
    <button type="button" className="pov-item" onClick={() => jump(item.anchor)} aria-label={`${item.name}: jump to changes`}>
      <span className="pov-item-icon">{item.itemId != null
        ? <ItemIcon id={item.itemId} name={item.name} mode="desktop" />
        : <span className="pov-initial" aria-hidden>{item.name.slice(0, 1)}</span>}</span>
      <span className="pov-name">{item.name}</span>
    </button>
  </li>)}</ul></div>;
}

function Links({ title, links }: { title: string; links: OverviewLink[] }) {
  if (!links.length) return null;
  return <div className="pov-links"><h3>{title}</h3><ul>{links.map(link => <li key={link.anchor}>
    <button type="button" onClick={() => jump(link.anchor)}>{link.name}</button>
  </li>)}</ul></div>;
}

/** The patch at a glance: champions by verdict, then items and systems; everything jumps to its card. */
export function PatchOverview({ sections, champions }: { sections: PatchSection[]; champions: { id: number; name: string }[] }) {
  const overview = useMemo(() => patchOverview(sections, champions), [sections, champions]);
  if (!GROUPS.some(group => overview[group.key].length) && !overview.items.length && !overview.systems.length) return null;
  return <section className="pov" aria-labelledby="pov-title">
    <h2 id="pov-title" className="pov-title">Patch overview</h2>
    <div className="pov-body">
      <div className="pov-champions">{GROUPS.map(group => overview[group.key].length > 0 && <div key={group.key} className={`pov-group ${group.tone}`}>
        <h3><span aria-hidden>{group.mark}</span>{group.title}<small>{overview[group.key].length}</small></h3>
        <ul>{overview[group.key].map(champion => <Champion key={champion.anchor} champion={champion} />)}</ul>
      </div>)}</div>
      {(overview.items.length > 0 || overview.systems.length > 0) && <div className="pov-side">
        <Items items={overview.items} />
        <Links title="Systems" links={overview.systems} />
      </div>}
    </div>
  </section>;
}
