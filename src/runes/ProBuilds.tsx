import { useState } from "react";
import { ItemIcon } from "./ItemIcon";
import { RuneIcon } from "./RuneIcon";
import { RoleIcon } from "./RoleIcon";
import { SpellIcon } from "./SpellIcon";
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
  loading: boolean;
  error: string | null;
  busy: boolean;
  /** The page currently applied in League, highlighted on a matching card. */
  active: Selection | null;
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
  const subtitle = [build.team, build.league].filter(Boolean).join(" · ");
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
      {(build.itemOrder.length > 0 || build.finalItems.length > 0) && (
        <>
          <div className="pro-card-actions">
            {build.itemOrder.length > 0 && <button
              type="button"
              className="pro-card-toggle"
              disabled={busy}
              aria-expanded={open}
              onClick={() => setOpen((value) => !value)}
            >
              <span>Item order</span>
              <ChevronIcon open={open} />
            </button>}
            {build.finalItems.length > 0 && <button
              type="button"
              className="item-set-import"
              disabled={busy}
              onClick={() => onImportItems(build)}
              aria-label={`Add ${build.proName}'s item build to the League shop`}
            >Add to shop</button>}
          </div>
          {open && (
            <ol className="pro-card-order">
              {build.itemOrder.map((entry, index) => (
                <li key={`${entry.itemId}-${index}`} className="pro-order-row">
                  <span className="pro-order-minute">{fmtBuildMinute(entry.minute)}</span>
                  <ItemIcon id={entry.itemId} name={entry.name} mode={mode} className="pro-order-icon" />
                  <span className="pro-order-name">{entry.name}</span>
                </li>
              ))}
            </ol>
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
  loading,
  error,
  busy,
  active,
  onImport,
  onImportItems,
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
