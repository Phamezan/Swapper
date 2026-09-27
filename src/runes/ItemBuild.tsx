import { useState } from "react";
import { ItemIcon } from "./ItemIcon";
import { fmtGames, fmtPct, type BuildGroup, type ItemBuildView } from "./types";

function BuildRow({ group, mode }: { group: BuildGroup; mode: "desktop" | "remote" }) {
  return (
    <div className="build-row">
      <span className="build-row-label">{group.label}</span>
      <span className="build-row-items">
        {group.items.map((item) => (
          <ItemIcon
            key={item.id}
            id={item.id}
            name={item.name}
            mode={mode}
            className="build-item"
          />
        ))}
      </span>
      <span className="build-row-stats">
        <span className="build-row-win">{fmtPct(group.winPct)}</span>
        <span className="build-row-games">{fmtGames(group.play)}</span>
      </span>
    </div>
  );
}

type Props = {
  build: ItemBuildView;
  mode: "desktop" | "remote";
};

/** The recommended item build under the preset cards: the top starter, boots and
 *  core, then a couple of alternative cores behind a small toggle, then the
 *  popular late items. Display only; Swapper does not import items. */
export function ItemBuild({ build, mode }: Props) {
  const [showMore, setShowMore] = useState(false);
  const primary = [build.starter, build.boots, build.core].filter(
    (group): group is BuildGroup => group !== null,
  );
  const hasAlternatives = build.coreAlternatives.length > 0;
  const groups = showMore ? [...primary, ...build.coreAlternatives] : primary;

  return (
    <section className="item-build" aria-label="Recommended item build">
      <div className="item-build-head">
        <span className="item-build-title">Build</span>
        {hasAlternatives && (
          <button
            type="button"
            className="item-build-more"
            aria-expanded={showMore}
            onClick={() => setShowMore((open) => !open)}
          >
            {showMore ? "Fewer cores" : "More cores"}
          </button>
        )}
      </div>
      <div className="item-build-groups">
        {groups.map((group, index) => (
          <BuildRow key={`${group.label}-${index}`} group={group} mode={mode} />
        ))}
      </div>
      {build.late.length > 0 && (
        <div className="item-build-late">
          <span className="build-row-label">Late</span>
          <div className="build-late-items">
            {build.late.map((group) => {
              const item = group.items[0];
              if (!item) return null;
              return (
                <span key={item.id} className="build-late-option">
                  <ItemIcon id={item.id} name={item.name} mode={mode} className="build-item" />
                  <span className="build-late-win">{fmtPct(group.winPct)}</span>
                </span>
              );
            })}
          </div>
        </div>
      )}
    </section>
  );
}
