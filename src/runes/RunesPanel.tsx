import { useEffect, useState } from "react";
import { RuneIcon } from "./RuneIcon";
import {
  Rune,
  RuneTree,
  RunesView,
  Selection,
  selectionFromPreset,
  selectionFromApplied,
  sameSelection,
  togglePrimary,
  toggleSecondary,
  selectShard,
} from "./types";
import "./runes.css";

function CheckIcon({ size = 15 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2.6} strokeLinecap="round" strokeLinejoin="round" aria-hidden>
      <path d="M20 6 9 17l-5-5" />
    </svg>
  );
}

function ResetIcon({ size = 14 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2.2} strokeLinecap="round" strokeLinejoin="round" aria-hidden>
      <path d="M3.5 12a8.5 8.5 0 1 0 2.6-6.1" />
      <path d="M3 4v5h5" />
    </svg>
  );
}

function Spinner() {
  return <span className="runes-spinner" aria-hidden />;
}

type Props = {
  mode: "desktop" | "remote";
  view: RunesView | null;
  loading: boolean;
  busy: boolean;
  error: string | null;
  onApply: (selection: Selection, presetIndex: number | null) => void;
  onToggleAutoApply: (enabled: boolean) => void;
  onExit?: () => void;
};

const positionLabels: Record<string, string> = {
  top: "Top",
  jungle: "Jungle",
  mid: "Mid",
  adc: "Bot",
  support: "Support",
  none: "No role",
};

const modeLabels: Record<string, string> = {
  ranked: "Ranked",
  aram: "ARAM",
  arena: "Arena",
};

function fmtPct(value: number | null): string {
  if (value === null || Number.isNaN(value)) return "–";
  return `${value.toFixed(1)}%`;
}

function fmtGames(play: number): string {
  if (!play) return "–";
  if (play >= 1000) return `${(play / 1000).toFixed(1)}k`;
  return String(play);
}

function defaultSelection(view: RunesView): Selection | null {
  const primary = view.trees[0];
  if (!primary || primary.keystones.length === 0) return null;
  const secondary = view.trees.find((tree) => tree.id !== primary.id);
  if (!secondary) return null;
  const primaryRunes = primary.rows
    .map((row) => row.runes[0]?.id)
    .filter((id): id is number => id !== undefined);
  const secondaryRunes = secondary.rows
    .map((row) => row.runes[0]?.id)
    .filter((id): id is number => id !== undefined)
    .slice(0, 2);
  return {
    primaryPageId: primary.id,
    secondaryPageId: secondary.id,
    keystone: primary.keystones[0].id,
    primaryRunes,
    secondaryRunes,
    shards: view.shards.map((row) => row.runes[0]?.id ?? 0),
  };
}

function RuneTile({
  rune,
  mode,
  selected,
  disabled,
  onClick,
}: {
  rune: Rune;
  mode: "desktop" | "remote";
  selected: boolean;
  disabled: boolean;
  onClick: () => void;
}) {
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
      <span className="rune-tile-win">{fmtPct(rune.winPct)}</span>
      <span className="rune-tile-meta">
        {fmtPct(rune.pickPct)} · {fmtGames(rune.play)}
      </span>
    </button>
  );
}

export function RunesPanel({
  mode,
  view,
  loading,
  busy,
  error,
  onApply,
  onToggleAutoApply,
  onExit,
}: Props) {
  const [screen, setScreen] = useState<"presets" | "editor">("presets");
  const [selection, setSelection] = useState<Selection | null>(null);

  const championId = view?.championId ?? 0;
  useEffect(() => {
    setScreen("presets");
    setSelection(view?.applied ? selectionFromApplied(view.applied) : null);
    // Reset only when the champion changes, not on every statistics refresh.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [championId]);

  const appliedKey = view?.applied ? JSON.stringify(view.applied) : "";
  useEffect(() => {
    if (view?.applied) {
      const applied = selectionFromApplied(view.applied);
      setSelection((prev) => prev ?? applied);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [appliedKey]);

  if (loading && !view) {
    return (
      <div className={`runes-panel runes-${mode}`}>
        <div className="runes-status"><Spinner /> Loading runes…</div>
      </div>
    );
  }
  if (!view || view.championId === 0) {
    return (
      <div className={`runes-panel runes-${mode}`}>
        <div className="runes-empty">
          <h2>Runes</h2>
          <p>{view?.message ?? "Waiting for champion select."}</p>
        </div>
      </div>
    );
  }

  const primaryTree = view.trees.find((tree) => tree.id === selection?.primaryPageId) ?? view.trees[0];
  const secondaryTree =
    view.trees.find((tree) => tree.id === selection?.secondaryPageId) ??
    view.trees.find((tree) => tree.id !== primaryTree?.id);
  const active = view.applied ? selectionFromApplied(view.applied) : null;
  const dirty = selection !== null && !sameSelection(selection, active);
  const canEdit = selection !== null && primaryTree !== undefined && secondaryTree !== undefined;
  const applyIndex = selection && sameSelection(selection, active)
    ? view.applied?.presetIndex ?? null
    : null;

  // A stable non-null alias: TypeScript loses the narrowing above inside the
  // handlers declared below.
  const data = view;

  function choosePrimaryTree(tree: RuneTree) {
    if (!selection) return;
    const keystone = tree.keystones[0]?.id ?? selection.keystone;
    const primaryRunes = tree.rows
      .map((row) => row.runes[0]?.id)
      .filter((id): id is number => id !== undefined);
    let secondaryPageId = selection.secondaryPageId;
    let secondaryRunes = selection.secondaryRunes;
    if (tree.id === secondaryPageId) {
      const fallback = data.trees.find((entry) => entry.id !== tree.id);
      if (fallback) {
        secondaryPageId = fallback.id;
        secondaryRunes = fallback.rows
          .map((row) => row.runes[0]?.id)
          .filter((id): id is number => id !== undefined)
          .slice(0, 2);
      }
    }
    setSelection({ ...selection, primaryPageId: tree.id, keystone, primaryRunes, secondaryPageId, secondaryRunes });
  }

  function chooseSecondaryTree(tree: RuneTree) {
    if (!selection || tree.id === selection.primaryPageId) return;
    const secondaryRunes = tree.rows
      .map((row) => row.runes[0]?.id)
      .filter((id): id is number => id !== undefined)
      .slice(0, 2);
    setSelection({ ...selection, secondaryPageId: tree.id, secondaryRunes });
  }

  function startEditor() {
    setScreen("editor");
    setSelection((prev) =>
      prev ?? (data.applied ? selectionFromApplied(data.applied) : defaultSelection(data)),
    );
  }

  const sourceClass = view.source === "opgg" ? "is-live" : view.source === "lcu" ? "is-fallback" : "is-none";
  const roleLabel = view.position && view.position !== "none" ? positionLabels[view.position] : modeLabels[view.mode] ?? view.mode;

  return (
    <div className={`runes-panel runes-${mode}`}>
      <div className="runes-head">
        <div className="runes-title">
          <p>{view.championName || "Champion"} · {roleLabel}</p>
          <h2>Runes</h2>
        </div>
        {onExit && (
          <button type="button" className="runes-back" onClick={onExit} aria-label="Back to accounts">
            Accounts
          </button>
        )}
      </div>

      <div className="runes-tabs" role="tablist">
        <button type="button" role="tab" aria-selected={screen === "presets"} className={screen === "presets" ? "is-active" : ""} onClick={() => setScreen("presets")}>Presets</button>
        <button type="button" role="tab" aria-selected={screen === "editor"} className={screen === "editor" ? "is-active" : ""} onClick={startEditor}>Editor</button>
        <span className={`runes-source ${sourceClass}`}>{view.sourceLabel}</span>
      </div>

      <div className="runes-body">
        {error && <p className="runes-alert" role="alert">{error}</p>}
        {view.message && !error && <p className="runes-note">{view.message}</p>}
        {view.source === "opgg" && <p className="runes-note runes-approx">Win%, pick% and games are approximate, aggregated from op.gg builds.</p>}

        {screen === "presets" ? (
          <div className="runes-presets">
            {view.presets.length === 0 && (
              <p className="runes-note">
                {view.canApply ? "No presets for this role yet." : "No recommendations available. You can still build a page in the Editor."}
              </p>
            )}
            {view.presets.map((preset) => {
              const presetSelection = selectionFromPreset(preset);
              const isActive = sameSelection(presetSelection, active);
              return (
                <button
                  type="button"
                  key={preset.index}
                  className={`rune-preset ${isActive ? "is-active" : ""}`}
                  disabled={busy}
                  onClick={() => {
                    setSelection(presetSelection);
                    setScreen("editor");
                    onApply(presetSelection, preset.index);
                  }}
                >
                  <span className="rune-preset-head">
                    <RuneIcon id={preset.keystone} mode={mode} className="rune-preset-keystone" />
                    <span className="rune-preset-copy">
                      <strong>{preset.title}</strong>
                      <small>
                        {preset.play > 0
                          ? `${fmtPct(preset.winPct)} win · ${fmtGames(preset.play)} games`
                          : "League recommendation"}
                      </small>
                    </span>
                    {isActive && <span className="rune-preset-check"><CheckIcon size={15} /></span>}
                  </span>
                  <span className="rune-preset-icons">
                    {preset.primaryRunes.map((id) => <RuneIcon key={`p${id}`} id={id} mode={mode} />)}
                    <span className="rune-preset-divider" />
                    {preset.secondaryRunes.map((id) => <RuneIcon key={`s${id}`} id={id} mode={mode} />)}
                    <span className="rune-preset-divider" />
                    {preset.shards.map((id, index) => <RuneIcon key={`m${index}`} id={id} mode={mode} className="rune-preset-shard" />)}
                  </span>
                </button>
              );
            })}
          </div>
        ) : canEdit ? (
          <div className="runes-editor">
            <div className="rune-trees">
              <div className="rune-tree-picker" role="group" aria-label="Primary tree">
                {view.trees.map((tree) => (
                  <button
                    type="button"
                    key={`p-${tree.id}`}
                    className={tree.id === primaryTree.id ? "is-on" : ""}
                    title={tree.name}
                    disabled={busy}
                    onClick={() => choosePrimaryTree(tree)}
                  >
                    <RuneIcon id={tree.id} mode={mode} />
                  </button>
                ))}
              </div>
              <div className="rune-tree-picker" role="group" aria-label="Secondary tree">
                {view.trees.map((tree) => (
                  <button
                    type="button"
                    key={`s-${tree.id}`}
                    className={tree.id === secondaryTree.id ? "is-on" : ""}
                    title={tree.name}
                    disabled={busy || tree.id === primaryTree.id}
                    onClick={() => chooseSecondaryTree(tree)}
                  >
                    <RuneIcon id={tree.id} mode={mode} />
                  </button>
                ))}
              </div>
            </div>

            <div className="rune-grid">
              <div className="rune-col">
                <p className="rune-col-label">{primaryTree.name}</p>
                <div className="rune-row rune-row-keystones">
                  {primaryTree.keystones.map((rune) => (
                    <RuneTile key={rune.id} rune={rune} mode={mode} disabled={busy} selected={selection.keystone === rune.id} onClick={() => setSelection({ ...selection, keystone: rune.id })} />
                  ))}
                </div>
                {primaryTree.rows.map((row, index) => (
                  <div className="rune-row" key={`pr-${index}`}>
                    {row.runes.map((rune) => (
                      <RuneTile key={rune.id} rune={rune} mode={mode} disabled={busy} selected={selection.primaryRunes.includes(rune.id)} onClick={() => setSelection(togglePrimary(selection, primaryTree, rune.id))} />
                    ))}
                  </div>
                ))}
              </div>

              <div className="rune-col">
                <p className="rune-col-label">{secondaryTree.name}</p>
                {secondaryTree.rows.map((row, index) => (
                  <div className="rune-row" key={`sr-${index}`}>
                    {row.runes.map((rune) => (
                      <RuneTile key={rune.id} rune={rune} mode={mode} disabled={busy} selected={selection.secondaryRunes.includes(rune.id)} onClick={() => setSelection(toggleSecondary(selection, secondaryTree, rune.id))} />
                    ))}
                  </div>
                ))}
              </div>

              <div className="rune-col rune-col-shards">
                <p className="rune-col-label">Shards</p>
                {view.shards.map((row, index) => (
                  <div className="rune-row" key={`sh-${index}`}>
                    {row.runes.map((rune) => (
                      <RuneTile key={rune.id} rune={rune} mode={mode} disabled={busy} selected={selection.shards[index] === rune.id} onClick={() => setSelection(selectShard(selection, index, rune.id))} />
                    ))}
                  </div>
                ))}
              </div>
            </div>
          </div>
        ) : (
          <p className="runes-note">The rune grid is unavailable{view.message ? `: ${view.message}` : "."}</p>
        )}
      </div>

      <div className="runes-foot">
        <label className="runes-auto">
          <input type="checkbox" checked={view.autoApply} disabled={busy} onChange={(event) => onToggleAutoApply(event.target.checked)} />
          <span>Auto-apply top preset</span>
        </label>
        <div className="runes-actions">
          <button type="button" className="runes-reset" disabled={busy || !dirty || !selection} onClick={() => setSelection(active ?? defaultSelection(data))}>
            <ResetIcon size={14} /> Reset
          </button>
          <button
            type="button"
            className="runes-apply"
            disabled={busy || !selection || (!view.canApply && !dirty)}
            onClick={() => selection && onApply(selection, applyIndex)}
          >
            {busy ? <Spinner /> : <CheckIcon size={15} />} Apply page
          </button>
        </div>
      </div>
    </div>
  );
}
