import { ChampionIcon } from "./ChampionIcon";
import { ItemIcon } from "./ItemIcon";
import { SpellIcon } from "./SpellIcon";
import { fmtDelta, fmtGames, fmtPct } from "./types";
import type { KeystoneBuildView, MatchupView, Preset, Spell } from "./types";

export type BuildChoice = "generic" | "matchup";

export function matchupDifficulty(delta: number) {
  return delta <= -2.5 ? "Hard" : delta < -0.75 ? "Tough" : delta < 0.75 ? "Even" : delta < 2.5 ? "Favoured" : "Easy";
}

// The API exposes IDs, not categories. Only known lane boots qualify here;
// never infer lane value from completed core items or later-slot alternatives.
const LANE_BOOTS = new Set([1001, 3006, 3009, 3020, 3047, 3111, 3158]);

export function laneChanges(matchup: MatchupView, general: KeystoneBuildView | null, preset: Preset | undefined, spells: Spell[]) {
  if (matchup.fallback || !matchup.preset || !matchup.build || !general || !preset) return null;
  const usualItems = new Set([...general.starters, ...general.items].map((item) => item.id));
  const items = [...matchup.build.starters, ...matchup.build.items.filter((item) => LANE_BOOTS.has(item.id))]
    .filter((item, index, all) => !usualItems.has(item.id) && all.findIndex((other) => other.id === item.id) === index);
  return [
    ...matchup.preset.spells.filter((id, index, all) => !preset.spells.includes(id) && all.indexOf(id) === index)
      .map((id) => ({ id, name: spells.find((spell) => spell.id === id)?.name ?? "Summoner spell", kind: "spell" as const })),
    ...items.map((item) => ({ ...item, kind: "item" as const })),
  ];
}

type PickerProps = {
  mode: "desktop" | "remote";
  enemies: { id: number; name: string }[];
  slots: number;
  laner: number;
  selected: number;
  disabled: boolean;
  onSelect: (id: number) => void;
};

export function EnemyPicker({ mode, enemies, slots, laner, selected, disabled, onSelect }: PickerProps) {
  return (
    <div className="matchup-picker" role="group" aria-label="Choose your lane opponent">
      <small>ENEMY TEAM · TAP YOUR LANE OPPONENT</small>
      <div className="matchup-picker-row">
        {enemies.map((enemy) => (
          <button key={enemy.id} type="button" className={enemy.id === selected ? "is-active" : selected > 0 ? "is-dimmed" : ""}
            aria-label={`Lane opponent: ${enemy.name}`} aria-pressed={enemy.id === selected} disabled={disabled} onClick={() => onSelect(enemy.id)}>
            <ChampionIcon id={enemy.id} name={enemy.name} mode={mode} />
            <small>{enemy.name}</small>
            {enemy.id === laner && <span className="matchup-lane-tag">LANE</span>}
          </button>
        ))}
        {Array.from({ length: Math.max(0, slots - enemies.length) }, (_, index) => (
          <span key={index} className="matchup-picker-tbd" aria-label="Not picked yet">
            <span className="matchup-silhouette"><svg width="22" height="22" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><circle cx="12" cy="8" r="4.5" /><path d="M3.5 21c.6-4.4 4-7 8.5-7s7.9 2.6 8.5 7z" /></svg></span>
            <small>Picking…</small>
          </span>
        ))}
      </div>
    </div>
  );
}

type Props = {
  mode: "desktop" | "remote";
  matchup: MatchupView;
  championId: number;
  championName: string;
  lane: string;
  rank: string;
  general: KeystoneBuildView | null;
  preset: Preset | undefined;
  spells: Spell[];
};

export function MatchupBanner({ mode, matchup, championId, championName, lane, rank, general, preset, spells }: Props) {
  const { stats, enemyName } = matchup;
  const usable = !matchup.fallback && !!matchup.preset;
  const changes = laneChanges(matchup, general, preset, spells);
  const difficulty = stats ? matchupDifficulty(stats.delta) : null;
  return (
    <section className="matchup-banner" aria-label="Matchup">
      {stats && <>
        <div className="matchup-heading">
          <span className="matchup-portraits"><ChampionIcon id={championId} name={championName} mode={mode} /><b>VS</b><ChampionIcon id={matchup.enemyChampionId} name={enemyName} mode={mode} /></span>
          <div className="matchup-title"><strong>{championName} vs {enemyName}</strong><small>{lane} · {rank} · {fmtGames(stats.games)} games</small></div>
          <span className={`matchup-difficulty is-${difficulty?.toLowerCase()}`}>{difficulty}</span>
        </div>
        <div className="matchup-stat-tiles">
          <div><small>YOUR WIN RATE</small><div><strong>{fmtPct(stats.winPct)}</strong> <span className={`matchup-delta ${stats.delta >= 0 ? "is-up" : "is-down"}`}>{fmtDelta(stats.delta)} vs avg</span></div></div>
          <div><small>{championName.toUpperCase()} AVERAGE</small><div><strong>{fmtPct(stats.avgWinPct)}</strong></div></div>
        </div>
      </>}
      {!usable ? <p className="runes-note">{stats
        ? `Only ${stats.games} games for this matchup, showing the general build.`
        : `No ${lane.toLowerCase()} data vs ${enemyName}. Showing the general build.`}</p>
        : changes === null ? <p className="runes-note">Lane comparison unavailable until both builds are loaded.</p>
        : changes.length === 0 ? <p className="runes-note">Nothing changes in lane: same spells, start and boots as usual.</p>
        : <div className="matchup-high-value"><small>HIGH VALUE IN THIS MATCHUP</small><div>{changes.map((item) => (
          <span key={`${item.kind}-${item.id}`}>{item.kind === "spell" ? <SpellIcon id={item.id} mode={mode} /> : <ItemIcon id={item.id} name={item.name} mode={mode} />}{item.name}</span>
        ))}</div></div>}
    </section>
  );
}
