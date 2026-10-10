import { useState } from "react";
import { ChampionIcon } from "./ChampionIcon";
import { ItemIcon } from "./ItemIcon";
import { RuneIcon } from "./RuneIcon";
import { SpellIcon } from "./SpellIcon";
import { fmtDelta, fmtPct, tierClass } from "./types";
import type { ChampionBuild, ChampionOverviewView } from "./types";

type Props = {
  mode: "desktop" | "remote";
  championId: number;
  name: string;
  view: ChampionOverviewView | null;
  loading: boolean;
};

/** Tier, rates, lane rank and damage split for the searched champion, with a
 *  collapsed read-only build below. The build comes from the same page, so
 *  expanding it fetches nothing. */
export function ChampionOverview({ mode, championId, name, view, loading }: Props) {
  const [showBuild, setShowBuild] = useState(false);

  if (loading && !view) return <span className="overview-skeleton" aria-hidden />;
  const stats = view?.stats;
  if (!view || !stats) return <p className="runes-note">Overview: TBD</p>;
  const damage = view.damage;

  return (
    <section className="overview" aria-label="Champion overview">
      <div className="overview-head">
        <span className="overview-icon">
          <ChampionIcon key={championId} id={championId} name={name} mode={mode} />
        </span>
        <div className="overview-rates">
          <strong className="overview-name">
            {name} <span className={`tier-text ${tierClass(stats.tier)}`}>{stats.tier}</span>
          </strong>
          <strong>
            {fmtPct(stats.winPct)}{" "}
            <small className={stats.delta >= 0 ? "is-up" : "is-down"}>{fmtDelta(stats.delta)} vs avg</small>
          </strong>
          <small>
            {fmtPct(stats.pickPct)} pick · {fmtPct(stats.banPct)} ban
          </small>
          <small>
            Rank {stats.rank} of {stats.rankTotal} · Patch {stats.patch}
          </small>
        </div>
      </div>
      {damage && (
        <div
          className="damage-bar"
          role="img"
          aria-label={`Damage: ${damage.physical.toFixed(0)}% physical, ${damage.magic.toFixed(0)}% magic, ${damage.trueDamage.toFixed(0)}% true`}
          title={`Physical ${damage.physical.toFixed(0)}% · Magic ${damage.magic.toFixed(0)}% · True ${damage.trueDamage.toFixed(0)}%`}
        >
          <span className="is-physical" style={{ width: `${damage.physical}%` }} />
          <span className="is-magic" style={{ width: `${damage.magic}%` }} />
          <span className="is-true" style={{ width: `${damage.trueDamage}%` }} />
        </div>
      )}
      <button
        type="button"
        className="overview-toggle"
        aria-expanded={showBuild}
        onClick={() => setShowBuild(!showBuild)}
      >
        Build {showBuild ? "▴" : "▾"}
      </button>
      {showBuild && (view.build ? <BuildSummary build={view.build} mode={mode} /> : <p className="runes-note">TBD</p>)}
    </section>
  );
}

function BuildSummary({ build, mode }: { build: ChampionBuild; mode: "desktop" | "remote" }) {
  const runes = build.runes;
  const priority = build.items.skillPriority;
  return (
    <div className="build-summary">
      {runes && (
        <div className="build-row">
          <RuneIcon id={runes.keystone} mode={mode} className="build-keystone" />
          {runes.primaryRunes.map((id) => <RuneIcon key={`p${id}`} id={id} mode={mode} />)}
          <span className="build-divider" />
          {runes.secondaryRunes.map((id) => <RuneIcon key={`s${id}`} id={id} mode={mode} />)}
        </div>
      )}
      {build.spells.length === 2 && (
        <div className="build-row">
          {build.spells.map((id, index) => <SpellIcon key={`sp${index}`} id={id} mode={mode} />)}
        </div>
      )}
      <div className="build-row">
        {build.items.items.map((item) => (
          <ItemIcon key={item.id} id={item.id} name={item.name} mode={mode} />
        ))}
      </div>
      {(priority || build.skillOrder) && (
        <p className="build-skills">
          {priority && <span>Max {priority.split("").join(" > ")}</span>}
          {build.skillOrder && <small>{build.skillOrder.split("").join(" ")}</small>}
        </p>
      )}
    </div>
  );
}
