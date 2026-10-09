import { useEffect, useRef, useState } from "react";
import { ChampionIcon } from "./ChampionIcon";
import { fmtDelta, fmtGames, fmtPct } from "./types";
import type { CounterRow, MatchupView } from "./types";

/** Cards per strip, so the DOM stays small. */
const MAX_CARDS = 15;
const SKELETON_CARDS = 6;

type StripProps = {
  title: string;
  subtitle: string;
  /** Colour of the win rate: red for toughest, green for easiest. */
  tone: "loss" | "win";
  rows: CounterRow[] | undefined;
  loading: boolean;
  mode: "desktop" | "remote";
  tier: string;
  championId: number;
  lane: string;
  onLoadMatchup: (
    championId: number,
    enemyChampionId: number,
    position: string,
    tier: string,
  ) => Promise<MatchupView | null>;
};

/** A sideways strip of cards, each the searched champion's win rate into one
 *  opponent. Tapping a card shows that matchup's details underneath. */
export function CardStrip({ title, subtitle, tone, rows, loading, mode, tier, championId, lane, onLoadMatchup }: StripProps) {
  const [open, setOpen] = useState<number | null>(null);
  // The detail remembers which card it belongs to, and only the latest tap's
  // answer is kept, so a slow earlier request cannot show under another card.
  const [detail, setDetail] = useState<{ rowId: number; view: MatchupView | null } | null>(null);
  const latest = useRef(0);

  useEffect(() => {
    latest.current += 1;
    setOpen(null);
    setDetail(null);
  }, [championId, lane, tier]);

  function toggle(row: CounterRow) {
    if (open === row.championId) {
      setOpen(null);
      return;
    }
    const request = ++latest.current;
    setOpen(row.championId);
    setDetail(null);
    // The tapped counter is the champion; the searched champion is its enemy.
    const settle = (view: MatchupView | null) => {
      if (request === latest.current) setDetail({ rowId: row.championId, view });
    };
    onLoadMatchup(row.championId, championId, lane, tier)
      .then(settle)
      .catch(() => settle(null));
  }

  const opened = (rows ?? []).find((row) => row.championId === open);
  return (
    <section className="counter-strip" aria-label={title}>
      <h3>{title}</h3>
      <p>{subtitle}</p>
      <div className="counter-cards">
        {loading && !rows
          ? Array.from({ length: SKELETON_CARDS }, (_, index) => (
              <span key={index} className="counter-card-skeleton" aria-hidden />
            ))
          : (rows ?? []).slice(0, MAX_CARDS).map((row) => (
              <button
                key={row.championId}
                type="button"
                className={`counter-card ${row.lowSample ? "is-low" : ""}`}
                aria-expanded={open === row.championId}
                title={row.lowSample ? `${row.name}: low sample, treat with care` : row.name}
                onClick={() => toggle(row)}
              >
                <ChampionIcon id={row.championId} name={row.name} mode={mode} />
                <span className="counter-card-name">{row.name}</span>
                <span className={`counter-card-rate is-${tone}`}>{fmtPct(row.winPct)}</span>
                <small>{row.games.toLocaleString()} matches</small>
              </button>
            ))}
      </div>
      {!loading && rows && rows.length === 0 && <p className="runes-note">TBD</p>}
      {opened && (
        <p className="runes-note counter-detail">
          {detail?.rowId !== opened.championId
            ? "Loading matchup…"
            : detail.view?.stats
              ? `${opened.name} vs ${detail.view.enemyName}: ${fmtPct(detail.view.stats.winPct)} · ${fmtGames(detail.view.stats.games)} games · ${fmtDelta(detail.view.stats.delta)} vs avg`
              : "TBD"}
        </p>
      )}
    </section>
  );
}
