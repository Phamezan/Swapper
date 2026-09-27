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

function RuneTile({
  rune,
  mode,
  selected,
  disabled,
  onClick,
}: {
  rune: Rune;
  mode: Mode;
  selected: boolean;
  disabled: boolean;
  onClick: () => void;
}) {
  const hasStats = rune.winPct !== null || rune.pickPct !== null || rune.play > 0;
  return (
    <button
      type="button"
      className={`rune-tile ${selected ? "is-on" : ""}`}
      disabled={disabled}
      onClick={onClick}
      title={rune.name}
      aria-pressed={selected}
      aria-label={`${rune.name}, ${selected ? "selected" : "not selected"}`}
    >
      <RuneIcon id={rune.id} mode={mode} className="rune-tile-icon" />
      <span className="rune-tile-name">{rune.name}</span>
      {hasStats && rune.winPct !== null && <span className="rune-tile-win">{fmtPct(rune.winPct)}</span>}
      {hasStats && rune.pickPct !== null && rune.play > 0 && (
        <span className="rune-tile-meta">
          {fmtPct(rune.pickPct)} · {fmtGames(rune.play)}
        </span>
      )}
    </button>
  );
}

function TreePicker({
  trees,
  mode,
  ariaLabel,
  activeId,
  disabledId,
  busy,
  onPick,
}: {
  trees: RuneTree[];
  mode: Mode;
  ariaLabel: string;
  activeId: number;
  disabledId?: number;
  busy: boolean;
  onPick: (tree: RuneTree) => void;
}) {
  return (
    <div className="rune-tree-picker" role="group" aria-label={ariaLabel}>
      {trees.map((tree) => (
        <button
          type="button"
          key={tree.id}
          className={tree.id === activeId ? "is-on" : ""}
          title={tree.name}
          disabled={busy || tree.id === disabledId}
          onClick={() => onPick(tree)}
        >
          <RuneIcon id={tree.id} mode={mode} />
          {mode === "remote" && <span className="rune-tree-option-name">{tree.name}</span>}
        </button>
      ))}
    </div>
  );
}

function Keystones({
  tree,
  mode,
  selection,
  busy,
  onChange,
}: {
  tree: RuneTree;
  mode: Mode;
  selection: Selection;
  busy: boolean;
  onChange: (selection: Selection) => void;
}) {
  return (
    <div className="rune-row rune-row-keystones">
      {tree.keystones.map((rune) => (
        <RuneTile
          key={rune.id}
          rune={rune}
          mode={mode}
          disabled={busy}
          selected={selection.keystone === rune.id}
          onClick={() => onChange({ ...selection, keystone: rune.id })}
        />
      ))}
    </div>
  );
}

function PrimaryRows({
  tree,
  mode,
  selection,
  busy,
  onChange,
}: {
  tree: RuneTree;
  mode: Mode;
  selection: Selection;
  busy: boolean;
  onChange: (selection: Selection) => void;
}) {
  return (
    <>
      {tree.rows.map((row, index) => (
        <div className="rune-row" key={`pr-${index}`}>
          {row.runes.map((rune) => (
            <RuneTile
              key={rune.id}
              rune={rune}
              mode={mode}
              disabled={busy}
              selected={selection.primaryRunes.includes(rune.id)}
              onClick={() => onChange(togglePrimary(selection, tree, rune.id))}
            />
          ))}
        </div>
      ))}
    </>
  );
}

function SecondaryRows({
  tree,
  mode,
  selection,
  busy,
  onChange,
}: {
  tree: RuneTree;
  mode: Mode;
  selection: Selection;
  busy: boolean;
  onChange: (selection: Selection) => void;
}) {
  return (
    <>
      {tree.rows.map((row, index) => (
        <div className="rune-row" key={`sr-${index}`}>
          {row.runes.map((rune) => (
            <RuneTile
              key={rune.id}
              rune={rune}
              mode={mode}
              disabled={busy}
              selected={selection.secondaryRunes.includes(rune.id)}
              onClick={() => onChange(toggleSecondary(selection, tree, rune.id))}
            />
          ))}
        </div>
      ))}
    </>
  );
}

function ShardOptions({
  row,
  index,
  mode,
  selection,
  busy,
  onChange,
}: {
  row: RuneRow;
  index: number;
  mode: Mode;
  selection: Selection;
  busy: boolean;
  onChange: (selection: Selection) => void;
}) {
  return (
    <div className="rune-row">
      {row.runes.map((rune) => (
        <RuneTile
          key={rune.id}
          rune={rune}
          mode={mode}
          disabled={busy}
          selected={selection.shards[index] === rune.id}
          onClick={() => onChange(selectShard(selection, index, rune.id))}
        />
      ))}
    </div>
  );
}

function SelectionSummary({
  view,
  mode,
  selection,
}: {
  view: RunesView;
  mode: Mode;
  selection: Selection;
}) {
  const keystone = view.trees
    .flatMap((tree) => tree.keystones)
    .find((rune) => rune.id === selection.keystone);
  const secondaryTree = view.trees.find((tree) => tree.id === selection.secondaryPageId);
  return (
    <div className="rune-summary" aria-label="Current selection">
      <span className="rune-summary-item">
        {keystone && <RuneIcon id={keystone.id} mode={mode} className="rune-summary-icon" />}
        <span className="rune-summary-copy">
          <small>{keystone?.name ?? "Keystone"}</small>
          <small>{secondaryTree?.name ?? "Secondary"}</small>
        </span>
      </span>
      <span className="rune-summary-shards">
        {selection.shards.map(
          (id, index) => id > 0 && <RuneIcon key={index} id={id} mode={mode} className="rune-summary-shard" />,
        )}
      </span>
    </div>
  );
}

type Step = "primary" | "secondary" | "shards";

const STEPS: { id: Step; label: string }[] = [
  { id: "primary", label: "Primary" },
  { id: "secondary", label: "Secondary" },
  { id: "shards", label: "Shards" },
];

export function RuneEditor({ mode, view, selection, busy, onChange }: Props) {
  const [step, setStep] = useState<Step>("primary");

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

  if (mode === "desktop") {
    return (
      <div className="runes-editor">
        <div className="rune-trees">
          <TreePicker
            trees={view.trees}
            mode={mode}
            ariaLabel="Primary tree"
            activeId={primaryTree.id}
            busy={busy}
            onPick={choosePrimaryTree}
          />
          <TreePicker
            trees={view.trees}
            mode={mode}
            ariaLabel="Secondary tree"
            activeId={secondaryTree.id}
            disabledId={primaryTree.id}
            busy={busy}
            onPick={chooseSecondaryTree}
          />
        </div>

        <div className="rune-grid">
          <div className="rune-col">
            <p className="rune-col-label">{primaryTree.name}</p>
            <Keystones tree={primaryTree} mode={mode} selection={selection} busy={busy} onChange={onChange} />
            <PrimaryRows tree={primaryTree} mode={mode} selection={selection} busy={busy} onChange={onChange} />
          </div>

          <div className="rune-col">
            <p className="rune-col-label">{secondaryTree.name}</p>
            <SecondaryRows tree={secondaryTree} mode={mode} selection={selection} busy={busy} onChange={onChange} />
          </div>

          <div className="rune-col rune-col-shards">
            <p className="rune-col-label">Shards</p>
            {view.shards.map((row, index) => (
              <ShardOptions
                key={`sh-${index}`}
                row={row}
                index={index}
                mode={mode}
                selection={selection}
                busy={busy}
                onChange={onChange}
              />
            ))}
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="runes-editor runes-editor-remote">
      <SelectionSummary view={view} mode={mode} selection={selection} />

      <div className="rune-steps" role="tablist" aria-label="Rune editor step">
        {STEPS.map((entry) => (
          <button
            type="button"
            role="tab"
            key={entry.id}
            aria-selected={step === entry.id}
            className={step === entry.id ? "is-active" : ""}
            onClick={() => setStep(entry.id)}
          >
            {entry.label}
          </button>
        ))}
      </div>

      {step === "primary" && (
        <div className="rune-step">
          <p className="rune-step-title">Primary tree</p>
          <TreePicker
            trees={view.trees}
            mode={mode}
            ariaLabel="Primary tree"
            activeId={primaryTree.id}
            busy={busy}
            onPick={choosePrimaryTree}
          />
          <div className="rune-col">
            <p className="rune-col-label">{primaryTree.name}</p>
            <Keystones tree={primaryTree} mode={mode} selection={selection} busy={busy} onChange={onChange} />
            <PrimaryRows tree={primaryTree} mode={mode} selection={selection} busy={busy} onChange={onChange} />
          </div>
        </div>
      )}

      {step === "secondary" && (
        <div className="rune-step">
          <p className="rune-step-title">Secondary tree</p>
          <TreePicker
            trees={view.trees.filter((tree) => tree.id !== primaryTree.id)}
            mode={mode}
            ariaLabel="Secondary tree"
            activeId={secondaryTree.id}
            busy={busy}
            onPick={chooseSecondaryTree}
          />
          <p className="rune-step-hint">Pick 2 runes from different rows.</p>
          <div className="rune-col">
            <p className="rune-col-label">{secondaryTree.name}</p>
            <SecondaryRows tree={secondaryTree} mode={mode} selection={selection} busy={busy} onChange={onChange} />
          </div>
        </div>
      )}

      {step === "shards" && (
        <div className="rune-step">
          <p className="rune-step-title">Stat shards</p>
          {view.shards.map((row, index) => (
            <div className="rune-shard-row" key={`sh-${index}`}>
              <p className="rune-col-label">{row.label || "Shard"}</p>
              <ShardOptions
                row={row}
                index={index}
                mode={mode}
                selection={selection}
                busy={busy}
                onChange={onChange}
              />
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
