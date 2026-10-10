import { useEffect, useState } from "react";
import { ChampionIcon } from "./ChampionIcon";
import { CachedBadge } from "./CachedBadge";
import { fmtGames, fmtPct, tierClass } from "./types";
import type { ChampionOption, TierListView } from "./types";

/** Rows shown at first, and added by each "Show more". */
const PAGE_SIZE = 30;
const SKELETON_ROWS = 8;

type Props = {
  mode: "desktop" | "remote";
  /** Swapper role: top, jungle, mid, adc or support. */
  position: string;
  tier: string;
  onLoad: (position: string, tier: string) => Promise<TierListView>;
  onPick: (champion: ChampionOption) => void;
};

/** The strongest champions for a lane and rank bracket. Tapping a row opens
 *  that champion's matchups. */
export function TierList({ mode, position, tier, onLoad, onPick }: Props) {
  const [view, setView] = useState<TierListView | null>(null);
  const [failed, setFailed] = useState(false);
  const [shown, setShown] = useState(PAGE_SIZE);

  useEffect(() => {
    let live = true;
    setView(null);
    setFailed(false);
    setShown(PAGE_SIZE);
    onLoad(position, tier)
      .then((next) => {
        if (live) setView(next);
      })
      .catch(() => {
        if (live) setFailed(true);
      });
    return () => {
      live = false;
    };
    // The loader is a stable wrapper; only the filter values should refetch.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [position, tier]);

  if (failed || view?.unavailable) {
    return <p className="runes-note">The tier list is unavailable right now.</p>;
  }
  return (
    <section className="tier-list" aria-label="Tier list">
      <h3>Tier list</h3>
      <p>Search a champion, or tap one below.</p>
      {view?.stale && <CachedBadge stale updatedAt={view.updatedAt} />}
      {!view
        ? Array.from({ length: SKELETON_ROWS }, (_, index) => (
            <span key={index} className="tier-row-skeleton" aria-hidden />
          ))
        : view.rows.slice(0, shown).map((row) => (
            <button
              key={row.championId}
              type="button"
              className="tier-row"
              onClick={() => onPick({ id: row.championId, name: row.name })}
            >
              <span className={`tier-text ${tierClass(row.tier)}`}>{row.tier}</span>
              <ChampionIcon id={row.championId} name={row.name} mode={mode} />
              <strong>{row.name}</strong>
              <span className="tier-rate">{fmtPct(row.winPct)}</span>
              <small>
                <b>{fmtPct(row.pickPct)}</b> pick · <b>{fmtPct(row.banPct)}</b> ban · <b>{fmtGames(row.games)}</b> games
              </small>
            </button>
          ))}
      {view && view.rows.length === 0 && <p className="runes-note">TBD</p>}
      {view && view.rows.length > shown && (
        <button type="button" className="tier-more" onClick={() => setShown(shown + PAGE_SIZE)}>
          Show more
        </button>
      )}
    </section>
  );
}
