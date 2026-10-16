import { useEffect, useState, type ReactNode } from "react";
import { ChampionIcon } from "./ChampionIcon";
import { CachedBadge } from "./CachedBadge";
import { fmtExactGames, fmtPct, tierClass, wrTone } from "./types";
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

/** A bold value over a small caps label; `tone` colours pick and ban figures. */
export function Stat({ value, label, tone }: { value: ReactNode; label: string; tone?: "pick" | "ban" }) {
  return (
    <span>
      <b className={tone ? `v-${tone}` : undefined}>{value}</b>
      <i>{label}</i>
    </span>
  );
}

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
                <span className={`tier-rate wr-${wrTone(row.winPct)}`}>{fmtPct(row.winPct)}</span>
                <span className="stat-blk">
                  {row.games > 0 ? <Stat value={fmtExactGames(row.games)} label="games" /> : <small className="stat-note">Last patch</small>}
                </span>
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
                <span className={`tier-rate wr-${wrTone(row.winPct)}`}>{fmtPct(row.winPct)}</span>
                <span className="stat-blk">
                  <Stat value={fmtExactGames(row.games)} label="games" />
                  <Stat value={fmtPct(row.pickPct)} label="pick" tone="pick" />
                  <Stat value={fmtPct(row.banPct)} label="ban" tone="ban" />
                </span>
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
