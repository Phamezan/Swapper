import { fmtDelta, fmtGames, fmtPct } from "./types";
import type { MatchupView } from "./types";

export type BuildChoice = "generic" | "matchup";

type Props = {
  matchup: MatchupView;
  choice: BuildChoice;
  disabled: boolean;
  onChoose: (choice: BuildChoice) => void;
};

type PickerProps = {
  enemies: { id: number; name: string }[];
  slots: number;
  selected: number;
  disabled: boolean;
  onSelect: (id: number) => void;
};

/** The enemy team as tappable names; unrevealed slots are muted "TBD". The
 *  matchup loads only for the one chosen. */
export function EnemyPicker({ enemies, slots, selected, disabled, onSelect }: PickerProps) {
  const tbd = Math.max(0, slots - enemies.length);
  return (
    <div className="matchup-picker" role="group" aria-label="Choose your lane opponent">
      <small>Lane opponent</small>
      <div className="matchup-picker-row">
        {enemies.map((enemy) => (
          <button
            key={enemy.id}
            type="button"
            className={enemy.id === selected ? "is-active" : ""}
            aria-pressed={enemy.id === selected}
            disabled={disabled}
            onClick={() => onSelect(enemy.id)}
          >
            {enemy.name}
          </button>
        ))}
        {Array.from({ length: tbd }, (_, index) => (
          <span key={`tbd${index}`} className="matchup-picker-tbd">TBD</span>
        ))}
      </div>
    </div>
  );
}

/** The matchup against the enemy laner, with a switch between the generic
 *  preset list and the matchup build. A small sample is shown but never used:
 *  the matchup option stays off and the generic build is explained. */
export function MatchupBanner({ matchup, choice, disabled, onChoose }: Props) {
  const { stats, enemyName } = matchup;
  const usable = !matchup.fallback && matchup.preset !== null;
  return (
    <div className="matchup-banner">
      <div className="matchup-stats">
        <strong>vs {enemyName}</strong>
        {stats ? (
          <small>
            {fmtPct(stats.winPct)} · {fmtGames(stats.games)} games ·{" "}
            <span className={`matchup-delta ${stats.delta >= 0 ? "is-up" : "is-down"}`}>
              {fmtDelta(stats.delta)} vs avg
            </span>
          </small>
        ) : (
          <small>No matchup data</small>
        )}
      </div>
      <div className="matchup-toggle" role="group" aria-label="Choose build">
        <button
          type="button"
          className={choice === "generic" || !usable ? "is-active" : ""}
          aria-pressed={choice === "generic" || !usable}
          disabled={disabled}
          onClick={() => onChoose("generic")}
        >
          Generic
        </button>
        <button
          type="button"
          className={choice === "matchup" && usable ? "is-active" : ""}
          aria-pressed={choice === "matchup" && usable}
          disabled={disabled || !usable}
          onClick={() => onChoose("matchup")}
          title={`Runes, spells, items and skill order for ${enemyName}`}
        >
          vs {enemyName}
        </button>
      </div>
      {matchup.fallback && (
        <p className="runes-note">
          {stats
            ? `Low sample (${stats.games} games), showing generic build.`
            : "Matchup data unavailable, showing generic build."}
        </p>
      )}
    </div>
  );
}
