import { Fragment, useState } from "react";
import { ItemIcon } from "./ItemIcon";
import { RuneIcon } from "./RuneIcon";
import { RoleIcon } from "./RoleIcon";
import { SpellIcon } from "./SpellIcon";
import { TeamLogo } from "./TeamLogo";
import { CachedBadge } from "./CachedBadge";
import {
  fmtBuildMinute,
  isTrinketItem,
  ProBuild,
  ProBuildsView,
  Selection,
  selectionFromProBuild,
  sameSelection,
} from "./types";

type Props = {
  mode: "desktop" | "remote";
  view: ProBuildsView | null;
  /** Shown when there are no games and the backend gave no reason. */
  emptyMessage?: string;
  loading: boolean;
  error: string | null;
  busy: boolean;
  /** The page currently applied in League, highlighted on a matching card. */
  active: Selection | null;
  /** True when the OTP list (not the pro list) is shown. */
  otp: boolean;
  onOtpChange: (otp: boolean) => void;
  onImport: (build: ProBuild) => void;
  onImportItems: (build: ProBuild) => void;
  onLoadMore: () => void;
  onRetry: () => void;
};

function Spinner() {
  return <span className="runes-spinner" aria-hidden />;
}

function ChevronIcon({ open }: { open: boolean }) {
  return (
    <svg
      className={`pro-card-chevron ${open ? "is-open" : ""}`}
      width={13}
      height={13}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2.4}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      <path d="m6 9 6 6 6-6" />
    </svg>
  );
}

function kda(build: ProBuild): string {
  return `${build.kills}/${build.deaths}/${build.assists}`;
}

/** The ability rows of the skill-order grid, in display order. */
const SKILL_ROWS = ["Q", "W", "E", "R"] as const;

function ProBuildCard({
  build,
  mode,
  active,
  busy,
  onImport,
  onImportItems,
}: {
  build: ProBuild;
  mode: "desktop" | "remote";
  active: boolean;
  busy: boolean;
  onImport: (build: ProBuild) => void;
  onImportItems: (build: ProBuild) => void;
}) {
  const [open, setOpen] = useState(false);
  const hasDetails =
    build.completedItems.length > 0 || build.itemPath.length > 0 || build.skillOrder.length > 0;
  return (
    <div className={`pro-card ${active ? "is-active" : ""}`}>
      <button
        type="button"
        className="pro-card-main"
        disabled={busy}
        onClick={() => onImport(build)}
        aria-label={`Import ${build.proName}'s ${build.win ? "winning" : "losing"} ${build.roleLabel} page`}
      >
        <span className="pro-card-head">
          <TeamLogo team={build.team} size={26} mode={mode} className="pro-card-logo" />
          <RuneIcon id={build.keystone} mode={mode} className="pro-card-keystone" />
          <span className="pro-card-copy">
            <span className="pro-card-name">
              <strong>{build.proName || "Pro player"}</strong>
              {build.region && (
                <span
                  className="pro-card-region"
                  data-region={build.region}
                  title={build.league || undefined}
                >
                  {build.region}
                </span>
              )}
            </span>
            {build.otp ? (
              <span className="pro-card-otp">OTP</span>
            ) : (
              build.team && <small>{build.team}</small>
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
        {build.finalItems.length > 0 && (
          <span className="pro-card-items" aria-label="Final build">
            {build.finalItems.map((item) => (
              <ItemIcon
                key={item.id}
                id={item.id}
                name={item.name}
                mode={mode}
                className={`pro-card-item ${isTrinketItem(item.id) ? "is-trinket" : ""}`}
              />
            ))}
          </span>
        )}
      </button>
      {(hasDetails || build.finalItems.length > 0) && (
        <>
          <div className="pro-card-actions">
            {hasDetails && (
              <button
                type="button"
                className="pro-card-toggle"
                disabled={busy}
                aria-expanded={open}
                onClick={() => setOpen((value) => !value)}
              >
                <span>Build details</span>
                <ChevronIcon open={open} />
              </button>
            )}
            {build.finalItems.length > 0 && <button
              type="button"
              className="item-set-import"
              disabled={busy}
              onClick={() => onImportItems(build)}
              aria-label={`Add ${build.proName}'s item build to the League shop`}
            >Add to shop</button>}
          </div>
          {open && hasDetails && (
            <div className="pro-details">
              {build.completedItems.length > 0 && (
                <section className="pro-details-section">
                  <span className="pro-details-title">Completed build</span>
                  <div className="pro-chain">
                    {build.completedItems.map((item, index) => (
                      <Fragment key={`${item.id}-${index}`}>
                        {index > 0 && <span className="pro-chain-sep" aria-hidden>›</span>}
                        <ItemIcon
                          id={item.id}
                          name={item.name}
                          mode={mode}
                          className="pro-details-item"
                        />
                      </Fragment>
                    ))}
                  </div>
                </section>
              )}
              {build.itemPath.length > 0 && (
                <section className="pro-details-section">
                  <span className="pro-details-title">Item path</span>
                  <div className="pro-path">
                    {build.itemPath.map((group, groupIndex) => (
                      <div key={`${group.minute}-${groupIndex}`} className="pro-path-group">
                        <div className="pro-path-icons">
                          {group.items.map((item, index) => (
                            <span key={`${item.id}-${index}`} className="pro-path-cell">
                              <ItemIcon
                                id={item.id}
                                name={item.name}
                                mode={mode}
                                className="pro-path-icon"
                              />
                              {item.count > 1 && (
                                <span className="pro-path-count">{item.count}</span>
                              )}
                            </span>
                          ))}
                        </div>
                        <span className="pro-path-minute">{fmtBuildMinute(group.minute)}</span>
                      </div>
                    ))}
                  </div>
                </section>
              )}
              {build.skillOrder.length > 0 && (
                <section className="pro-details-section">
                  <span className="pro-details-title">Skill order</span>
                  <div className="pro-skill-scroll">
                    <div className="pro-skill-rows">
                      {SKILL_ROWS.map((letter) => (
                        <div key={letter} className="pro-skill-row">
                          <span className="pro-skill-label">{letter}</span>
                          {build.skillOrder.map((chosen, level) => (
                            <span
                              key={`${letter}-${level}`}
                              className={`pro-skill-cell ${chosen === letter ? "is-filled" : ""}`}
                            >
                              {chosen === letter ? level + 1 : ""}
                            </span>
                          ))}
                        </div>
                      ))}
                    </div>
                  </div>
                </section>
              )}
            </div>
          )}
        </>
      )}
    </div>
  );
}

/** Recent solo-queue games by pro players, from probuildstats/u.gg. Tapping a
 *  card imports that exact page through the same path a preset uses; the card
 *  also starts compact and can expand to show the item order. */
export function ProBuilds({
  mode,
  view,
  emptyMessage,
  loading,
  error,
  busy,
  active,
  otp,
  onOtpChange,
  onImport,
  onImportItems,
  onLoadMore,
  onRetry,
}: Props) {
  const toggle = (
    <div className="matchup-toggle pro-otp-toggle" role="tablist" aria-label="Pro builds source">
      <button
        type="button"
        role="tab"
        aria-selected={!otp}
        className={otp ? "" : "is-active"}
        onClick={() => onOtpChange(false)}
      >
        Pros
      </button>
      <button
        type="button"
        role="tab"
        aria-selected={otp}
        className={otp ? "is-active" : ""}
        onClick={() => onOtpChange(true)}
      >
        OTPs
      </button>
    </div>
  );
  if (error) {
    return (
      <div className="pro-list">
        {toggle}
        <p className="runes-alert" role="alert">{error}</p>
        <button type="button" className="pro-retry" onClick={onRetry}>Try again</button>
      </div>
    );
  }
  if (!view) {
    return (
      <div className="pro-list">
        {toggle}
        <div className="runes-status"><Spinner /> Loading pro builds…</div>
      </div>
    );
  }
  if (view.unavailable) {
    return (
      <div className="pro-list">
        {toggle}
        <p className="runes-note pro-unavailable">
          {view.message ?? "Pro builds unavailable."}
        </p>
        <button type="button" className="pro-retry" onClick={onRetry}>Try again</button>
      </div>
    );
  }
  if (view.matches.length === 0) {
    return (
      <div className="pro-list">
        {toggle}
        <p className="runes-note">
          {view.message ?? emptyMessage ?? "No recent pro games for this champion."}
        </p>
      </div>
    );
  }
  return (
    <div className="pro-list">
      {toggle}
      {view.stale && <CachedBadge stale updatedAt={view.updatedAt} />}
      {view.matches.map((build) => (
        <ProBuildCard
          key={build.matchId}
          build={build}
          mode={mode}
          busy={busy}
          active={active !== null && sameSelection(active, selectionFromProBuild(build))}
          onImport={onImport}
          onImportItems={onImportItems}
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
