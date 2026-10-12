import { Fragment, useEffect, useState } from "react";
import { ItemIcon } from "./ItemIcon";
import { CachedBadge } from "./CachedBadge";
import type { KeystoneBuildView } from "./types";

/** How many placeholder tiles the loading skeleton shows. */
const SKELETON_ITEMS = 6;
/** Above this sample size the games count is not worth showing. */
const SMALL_SAMPLE = 1000;

type Props = {
  mode: "desktop" | "remote";
  championId: number;
  position: string;
  tier: string;
  keystone: number;
  /** A build that is already loaded (the matchup build); skips the lookup. */
  preloaded?: KeystoneBuildView | null;
  /** Loads the keystone's build for the current champion, role and bracket. */
  onLoad: (
    championId: number,
    position: string,
    tier: string,
    keystone: number,
  ) => Promise<KeystoneBuildView | null>;
};

/** The row of 6 items people build with this preset's keystone, from
 *  lolalytics. The card renders immediately and the row fills in when the
 *  (cached, prefetched) build arrives, showing a skeleton until then. The row
 *  is hidden when there is no build; a failed lookup is simply nothing. */
export function PresetBuild({ mode, championId, position, tier, keystone, preloaded, onLoad }: Props) {
  const [build, setBuild] = useState<KeystoneBuildView | null>(preloaded ?? null);
  const [loading, setLoading] = useState(!preloaded);

  useEffect(() => {
    if (preloaded) {
      setBuild(preloaded);
      setLoading(false);
      return;
    }
    let live = true;
    setBuild(null);
    setLoading(true);
    onLoad(championId, position, tier, keystone)
      .then((next) => {
        if (live) setBuild(next);
      })
      .catch(() => {
        if (live) setBuild(null);
      })
      .finally(() => {
        if (live) setLoading(false);
      });
    return () => {
      live = false;
    };
    // The loader is a stable wrapper; only the filter values should refetch.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [championId, position, tier, keystone, preloaded]);

  if (loading && !build) {
    return (
      <span className="preset-build" aria-hidden>
        {Array.from({ length: SKELETON_ITEMS }, (_, index) => (
          <span key={index} className="preset-build-skeleton" />
        ))}
      </span>
    );
  }
  if (!build || build.items.length === 0) return null;

  // The first `coreLen` items are one observed 3-item core; the rest are
  // independent per-slot popularity picks, not a joint sample.
  const coreLen = build.coreLen ?? 0;
  const showCoreGames = coreLen > 0 && build.coreGames < SMALL_SAMPLE;

  return (
    <div className="preset-build-row">
      <span className="preset-build" aria-label="Item picks from LoLalytics: the observed core, then the most popular later-slot items">
        {build.items.map((item, index) => (
          <Fragment key={item.id}>
            {coreLen > 0 && index === coreLen && <span className="preset-build-divider" />}
            <ItemIcon id={item.id} name={item.name} mode={mode} />
          </Fragment>
        ))}
        {showCoreGames && (
          <span className="preset-build-games" title="Games for this exact 3-item core combination">
            {build.coreGames} core games
          </span>
        )}
        <CachedBadge stale={build.stale} updatedAt={build.updatedAt} className="preset-build-cached" />
      </span>
      {build.skillPriority && (
        <span
          className="preset-skill-priority"
          title={build.skillOrder ? `Skill order: ${build.skillOrder.split("").join(" ")}` : "Ability max order"}
          aria-label={`Max ${build.skillPriority.split("").join(", then ")}`}
        >
          {build.skillPriority.split("").map((key, index) => (
            <Fragment key={key}>
              {index > 0 && <span className="preset-skill-sep">›</span>}
              <span className="preset-skill-key">{key}</span>
            </Fragment>
          ))}
        </span>
      )}
    </div>
  );
}
