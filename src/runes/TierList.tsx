import { useEffect, useState } from "react";
import { ChampionIcon } from "./ChampionIcon";
import { CachedBadge } from "./CachedBadge";
import { fmtGames, fmtPct, tierClass } from "./types";
import type { ChampionOption, DuoListView, TierListView } from "./types";

/** Rows shown at first, and added by each "Show more". */
const PAGE_SIZE = 30;
const SKELETON_ROWS = 8;

type Props = {
  mode: "desktop" | "remote";
  /** Swapper role: top, jungle, mid, adc or support. */
  position: string;
  tier: string;
  onLoad: (position: string, tier: string) => Promise<TierListView>;
  onLoadDuos: (tier: string) => Promise<DuoListView>;
  onPick: (champion: ChampionOption) => void;
};

/** A loaded list, tagged so a stale one of the other kind is never drawn. */
type Loaded = { duos: false; list: TierListView } | { duos: true; list: DuoListView };

/** The strongest champions for a lane and rank bracket or, for the bot lane,
 *  the strongest ADC + Support pairs. Tapping a row opens that champion's
 *  matchups (for a duo, the one in the selected role). */
export function TierList({ mode, position, tier, onLoad, onLoadDuos, onPick }: Props) {
  const [loaded, setLoaded] = useState<Loaded | null>(null);
  const [failed, setFailed] = useState(false);
  const [shown, setShown] = useState(PAGE_SIZE);
  const [duoMode, setDuoMode] = useState(false);
  const botLane = position === "adc" || position === "support";
  const duos = botLane && duoMode;

  useEffect(() => {
    let live = true;
    setLoaded(null);
    setFailed(false);
    setShown(PAGE_SIZE);
    const load: Promise<Loaded> = duos
      ? onLoadDuos(tier).then((list) => ({ duos: true, list }))
      : onLoad(position, tier).then((list) => ({ duos: false, list }));
    load
      .then((next) => {
        if (live) setLoaded(next);
      })
      .catch(() => {
        if (live) setFailed(true);
      });
    return () => {
      live = false;
    };
    // The loaders are stable wrappers; only the filter values should refetch.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [position, tier, duos]);

  const view = loaded && loaded.duos === duos ? loaded : null;
  const toggle = botLane && (
    <div className="matchup-toggle tier-duo-toggle" role="tablist" aria-label="Tier list kind">
      <button type="button" role="tab" aria-selected={!duos} className={duos ? "" : "is-active"} onClick={() => setDuoMode(false)}>
        Champions
      </button>
      <button type="button" role="tab" aria-selected={duos} className={duos ? "is-active" : ""} onClick={() => setDuoMode(true)}>
        Duos
      </button>
    </div>
  );

  if (failed || view?.list.unavailable) {
    return (
      <section className="tier-list" aria-label="Tier list">
        {toggle}
        <p className="runes-note">The {duos ? "duo" : "tier"} list is unavailable right now.</p>
      </section>
    );
  }
  const total = view?.list.rows.length ?? 0;
  return (
    <section className="tier-list" aria-label="Tier list">
      <h3>{duos ? "Bot lane duos" : "Tier list"}</h3>
      <p>Search a champion, or tap one below.</p>
      {toggle}
      {view?.list.stale && <CachedBadge stale updatedAt={view.list.updatedAt} />}
      {!view
        ? Array.from({ length: SKELETON_ROWS }, (_, index) => (
            <span key={index} className="tier-row-skeleton" aria-hidden />
          ))
        : view.duos
          ? view.list.rows.slice(0, shown).map((row) => (
              <button
                key={`${row.adcId}-${row.supportId}`}
                type="button"
                className="tier-row tier-duo-row"
                onClick={() =>
                  onPick(position === "support"
                    ? { id: row.supportId, name: row.supportName }
                    : { id: row.adcId, name: row.adcName })
                }
              >
                <span className={`tier-text ${tierClass(row.tier)}`}>{row.tier}</span>
                <span className="tier-duo-icons">
                  <ChampionIcon id={row.adcId} name={row.adcName} mode={mode} />
                  <ChampionIcon id={row.supportId} name={row.supportName} mode={mode} />
                </span>
                <strong>{row.adcName} + {row.supportName}</strong>
                <span className="tier-rate">{fmtPct(row.winPct)}</span>
                <small>
                  {row.games > 0 ? <><b>{fmtGames(row.games)}</b> games together</> : "Last patch"}
                </small>
              </button>
            ))
          : view.list.rows.slice(0, shown).map((row) => (
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
      {view && total === 0 && <p className="runes-note">TBD</p>}
      {view && total > shown && (
        <button type="button" className="tier-more" onClick={() => setShown(shown + PAGE_SIZE)}>
          Show more
        </button>
      )}
    </section>
  );
}
