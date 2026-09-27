import { useEffect, useState } from "react";
import { ItemIcon } from "./ItemIcon";
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
export function PresetBuild({ mode, championId, position, tier, keystone, onLoad }: Props) {
  const [build, setBuild] = useState<KeystoneBuildView | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
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
  }, [championId, position, tier, keystone]);

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

  return (
    <span className="preset-build" aria-label="Common build">
      {build.items.map((item) => (
        <ItemIcon key={item.id} id={item.id} name={item.name} mode={mode} />
      ))}
      {build.games > 0 && build.games < SMALL_SAMPLE && (
        <span className="preset-build-games">{build.games} games</span>
      )}
    </span>
  );
}
