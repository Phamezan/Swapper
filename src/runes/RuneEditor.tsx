import { useState } from "react";
import { RuneIcon } from "./RuneIcon";
import {
  Rune,
  RuneRow,
  RuneTree,
  RunesView,
  Selection,
  fmtGames,
  fmtPct,
  selectShard,
  togglePrimary,
  toggleSecondary,
} from "./types";

type Mode = "desktop" | "remote";

type Props = {
  mode: Mode;
  view: RunesView;
  selection: Selection;
  busy: boolean;
  onChange: (selection: Selection) => void;
};

/** The tooltip over a rune: the name plus whatever statistics exist. Pick% and
 *  games stay out of the grid so the runes read like the League client. */
function statTitle(rune: Rune): string {
  const parts = [rune.name];
  if (rune.winPct !== null && rune.play > 0) parts.push(`Win ${fmtPct(rune.winPct)}`);
  if (rune.pickPct !== null && rune.play > 0) parts.push(`Pick ${fmtPct(rune.pickPct)}`);
  if (rune.play > 0) parts.push(`${fmtGames(rune.play)} games`);
  if (parts.length > 1) parts.push("approx., from op.gg");
  return parts.join(" · ");
}

/** One round rune icon: full colour with a gold ring when selected, dimmed and
 *  desaturated otherwise, like the League client. */
function RuneOrb({
  rune,
  mode,
  selected,
  disabled,
  showStats,
  keystone,
  onClick,
}: {
  rune: Rune;
  mode: Mode;
  selected: boolean;
  disabled: boolean;
  showStats: boolean;
  keystone?: boolean;
  onClick: () => void;
}) {
  const hasWin = rune.winPct !== null && rune.play > 0;
  const hasDetail = showStats && rune.pickPct !== null && rune.play > 0;
  return (
    <button
      type="button"
      className={`rune-orb ${selected ? "is-on" : ""} ${keystone ? "rune-orb-keystone" : ""}`}
      disabled={disabled}
      onClick={onClick}
      title={statTitle(rune)}
      aria-pressed={selected}
      aria-label={`${rune.name}, ${selected ? "selected" : "not selected"}`}
    >
      <span className="rune-orb-art">
        <RuneIcon id={rune.id} mode={mode} className="rune-orb-icon" />
      </span>
      {hasWin && <span className="rune-orb-win">{fmtPct(rune.winPct)}</span>}
      {hasDetail && (
        <span className="rune-orb-meta">
          {fmtPct(rune.pickPct)} · {fmtGames(rune.play)}
        </span>
      )}
    </button>
  );
}

/** A column header: the selected tree as a larger icon, then the other trees to
 *  switch to. Matches the two icon rows in the League client. */
function TreeHeader({
  label,
  tree,
  trees,
  mode,
  disabledId,
  busy,
  onPick,
}: {
  label: string;
  tree: RuneTree;
  trees: RuneTree[];
  mode: Mode;
  disabledId?: number;
  busy: boolean;
  onPick: (tree: RuneTree) => void;
}) {
  const options = trees.filter((entry) => entry.id !== tree.id && entry.id !== disabledId);
  return (
    <div className="rune-head">
      <div className="rune-head-label">
        <small>{label}</small>
        <strong>{tree.name}</strong>
      </div>
      <div className="rune-head-trees" role="group" aria-label={`Switch ${label.toLowerCase()} tree`}>
        <span className="rune-head-big" title={tree.name}>
          <RuneIcon id={tree.id} mode={mode} />
        </span>
        {options.map((entry) => (
          <button
            type="button"
            key={entry.id}
            className="rune-head-option"
            title={entry.name}
            disabled={busy}
            onClick={() => onPick(entry)}
          >
            <RuneIcon id={entry.id} mode={mode} />
          </button>
        ))}
      </div>
    </div>
  );
}

function RuneRows({
  tree,
  mode,
  selection,
  busy,
  showStats,
  variant,
  onChange,
}: {
  tree: RuneTree;
  mode: Mode;
  selection: Selection;
  busy: boolean;
  showStats: boolean;
  variant: "primary" | "secondary";
  onChange: (selection: Selection) => void;
}) {
  const selectedIds =
    variant === "primary" ? selection.primaryRunes : selection.secondaryRunes;
  return (
    <div className="rune-rows">
      {tree.rows.map((row, index) => (
        <div className="rune-row" key={`${variant}-${index}`}>
          {row.runes.map((rune) => (
            <RuneOrb
              key={rune.id}
              rune={rune}
              mode={mode}
              disabled={busy}
              showStats={showStats}
              selected={selectedIds.includes(rune.id)}
              onClick={() =>
                onChange(
                  variant === "primary"
                    ? togglePrimary(selection, tree, rune.id)
                    : toggleSecondary(selection, tree, rune.id),
                )
              }
            />
          ))}
        </div>
      ))}
    </div>
  );
}

function ShardRows({
  rows,
  mode,
  selection,
  busy,
  onChange,
}: {
  rows: RuneRow[];
  mode: Mode;
  selection: Selection;
  busy: boolean;
  onChange: (selection: Selection) => void;
}) {
  return (
    <div className="rune-shards">
      {rows.map((row, index) => (
        <div className="rune-row rune-row-shard" key={`sh-${index}`}>
          {row.runes.map((rune) => (
            <RuneOrb
              key={rune.id}
              rune={rune}
              mode={mode}
              disabled={busy}
              showStats={false}
              selected={selection.shards[index] === rune.id}
              onClick={() => onChange(selectShard(selection, index, rune.id))}
            />
          ))}
        </div>
      ))}
    </div>
  );
}

export function RuneEditor({ mode, view, selection, busy, onChange }: Props) {
  const [showStats, setShowStats] = useState(false);

  const primaryTree =
    view.trees.find((tree) => tree.id === selection.primaryPageId) ?? view.trees[0];
  const secondaryTree =
    view.trees.find((tree) => tree.id === selection.secondaryPageId) ??
    view.trees.find((tree) => tree.id !== primaryTree?.id);

  if (!primaryTree || !secondaryTree) {
    return <p className="runes-note">The rune grid is unavailable.</p>;
  }

  function choosePrimaryTree(tree: RuneTree) {
    const keystone = tree.keystones[0]?.id ?? selection.keystone;
    const primaryRunes = tree.rows
      .map((row) => row.runes[0]?.id)
      .filter((id): id is number => id !== undefined);
    let secondaryPageId = selection.secondaryPageId;
    let secondaryRunes = selection.secondaryRunes;
    if (tree.id === secondaryPageId) {
      const fallback = view.trees.find((entry) => entry.id !== tree.id);
      if (fallback) {
        secondaryPageId = fallback.id;
        secondaryRunes = fallback.rows
          .map((row) => row.runes[0]?.id)
          .filter((id): id is number => id !== undefined)
          .slice(0, 2);
      }
    }
    onChange({ ...selection, primaryPageId: tree.id, keystone, primaryRunes, secondaryPageId, secondaryRunes });
  }

  function chooseSecondaryTree(tree: RuneTree) {
    if (tree.id === selection.primaryPageId) return;
    const secondaryRunes = tree.rows
      .map((row) => row.runes[0]?.id)
      .filter((id): id is number => id !== undefined)
      .slice(0, 2);
    onChange({ ...selection, secondaryPageId: tree.id, secondaryRunes });
  }

  return (
    <div className="runes-editor">
      <div className="rune-editor-tools">
        <label className="rune-stats-toggle">
          <input
            type="checkbox"
            checked={showStats}
            onChange={(event) => setShowStats(event.target.checked)}
          />
          <span>Show stats</span>
        </label>
      </div>

      <div className="rune-board">
        <div className="rune-board-col rune-board-primary">
          <TreeHeader
            label="Primary"
            tree={primaryTree}
            trees={view.trees}
            mode={mode}
            busy={busy}
            onPick={choosePrimaryTree}
          />
          <p className="rune-keystones-label">Keystones</p>
          <div className="rune-keystones">
            {primaryTree.keystones.map((rune) => (
              <RuneOrb
                key={rune.id}
                rune={rune}
                mode={mode}
                keystone
                disabled={busy}
                showStats={showStats}
                selected={selection.keystone === rune.id}
                onClick={() => onChange({ ...selection, keystone: rune.id })}
              />
            ))}
          </div>
          <RuneRows
            tree={primaryTree}
            mode={mode}
            selection={selection}
            busy={busy}
            showStats={showStats}
            variant="primary"
            onChange={onChange}
          />
        </div>

        <div className="rune-board-col rune-board-secondary">
          <TreeHeader
            label="Secondary"
            tree={secondaryTree}
            trees={view.trees}
            mode={mode}
            disabledId={primaryTree.id}
            busy={busy}
            onPick={chooseSecondaryTree}
          />
          <RuneRows
            tree={secondaryTree}
            mode={mode}
            selection={selection}
            busy={busy}
            showStats={showStats}
            variant="secondary"
            onChange={onChange}
          />
          <ShardRows
            rows={view.shards}
            mode={mode}
            selection={selection}
            busy={busy}
            onChange={onChange}
          />
        </div>
      </div>
    </div>
  );
}
