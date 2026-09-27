import { RuneIcon } from "./RuneIcon";
import { RoleIcon } from "./RoleIcon";
import { SpellIcon } from "./SpellIcon";
import {
  ProBuild,
  ProBuildsView,
  Selection,
  selectionFromProBuild,
  sameSelection,
} from "./types";

type Props = {
  mode: "desktop" | "remote";
  view: ProBuildsView | null;
  loading: boolean;
  error: string | null;
  busy: boolean;
  /** The page currently applied in League, highlighted on a matching card. */
  active: Selection | null;
  onImport: (build: ProBuild) => void;
  onLoadMore: () => void;
  onRetry: () => void;
};

function Spinner() {
  return <span className="runes-spinner" aria-hidden />;
}

function kda(build: ProBuild): string {
  return `${build.kills}/${build.deaths}/${build.assists}`;
}

function ProBuildCard({
  build,
  mode,
  active,
  busy,
  onImport,
}: {
  build: ProBuild;
  mode: "desktop" | "remote";
  active: boolean;
  busy: boolean;
  onImport: (build: ProBuild) => void;
}) {
  const subtitle = [build.team, build.league].filter(Boolean).join(" · ");
  return (
    <button
      type="button"
      className={`pro-card ${active ? "is-active" : ""}`}
      disabled={busy}
      onClick={() => onImport(build)}
      aria-label={`Import ${build.proName}'s ${build.win ? "winning" : "losing"} ${build.roleLabel} page`}
    >
      <span className="pro-card-head">
        <RuneIcon id={build.keystone} mode={mode} className="pro-card-keystone" />
        <span className="pro-card-copy">
          <strong>{build.proName || "Pro player"}</strong>
          {build.otp ? (
            <span className="pro-card-otp">OTP</span>
          ) : (
            subtitle && <small>{subtitle}</small>
          )}
        </span>
        <span className={`pro-card-result ${build.win ? "is-win" : "is-loss"}`}>
          {build.win ? "Win" : "Loss"}
        </span>
      </span>
      <span className="pro-card-meta">
        <span className="pro-card-kda">{kda(build)}</span>
        <span className="pro-card-dot" />
        <span>{build.playedAgo}</span>
        <span className="pro-card-dot" />
        <span>Patch {build.patch}</span>
        <span className="pro-card-dot" />
        <span className="pro-card-role">
          <RoleIcon role={build.role || build.roleLabel} size={13} mode={mode} />
          {build.roleLabel}
        </span>
      </span>
      <span className="pro-card-icons">
        <RuneIcon id={build.primaryPageId} mode={mode} className="pro-card-tree" />
        {build.primaryRunes.map((id) => (
          <RuneIcon key={`p${id}`} id={id} mode={mode} />
        ))}
        <span className="pro-card-divider" />
        <RuneIcon id={build.secondaryPageId} mode={mode} className="pro-card-tree" />
        {build.secondaryRunes.map((id) => (
          <RuneIcon key={`s${id}`} id={id} mode={mode} />
        ))}
        <span className="pro-card-divider" />
        {build.shards.map((id, index) => (
          <RuneIcon key={`m${index}`} id={id} mode={mode} className="pro-card-shard" />
        ))}
        {build.spells.length === 2 && (
          <>
            <span className="pro-card-divider" />
            {build.spells.map((id, index) => (
              <SpellIcon key={`sp${index}`} id={id} mode={mode} className="pro-card-spell" />
            ))}
          </>
        )}
      </span>
    </button>
  );
}

/** Recent solo-queue games by pro players, from probuildstats/u.gg. Tapping a
 *  card imports that exact page through the same path a preset uses. */
export function ProBuilds({
  mode,
  view,
  loading,
  error,
  busy,
  active,
  onImport,
  onLoadMore,
  onRetry,
}: Props) {
  if (error) {
    return (
      <div className="pro-list">
        <p className="runes-alert" role="alert">{error}</p>
        <button type="button" className="pro-retry" onClick={onRetry}>Try again</button>
      </div>
    );
  }
  if (!view) {
    return <div className="runes-status"><Spinner /> Loading pro builds…</div>;
  }
  if (view.unavailable) {
    return (
      <div className="pro-list">
        <p className="runes-note pro-unavailable">
          {view.message ?? "Pro builds unavailable."}
        </p>
        <button type="button" className="pro-retry" onClick={onRetry}>Try again</button>
      </div>
    );
  }
  if (view.matches.length === 0) {
    return (
      <p className="runes-note">
        {view.message ?? "No recent pro games for this champion."}
      </p>
    );
  }
  return (
    <div className="pro-list">
      {view.matches.map((build) => (
        <ProBuildCard
          key={build.matchId}
          build={build}
          mode={mode}
          busy={busy}
          active={active !== null && sameSelection(active, selectionFromProBuild(build))}
          onImport={onImport}
        />
      ))}
      {view.hasMore && (
        <button type="button" className="pro-more" disabled={loading} onClick={onLoadMore}>
          {loading ? <Spinner /> : null} Load more games
        </button>
      )}
      {!view.hasMore && <p className="runes-note pro-end">End of recent games.</p>}
    </div>
  );
}
