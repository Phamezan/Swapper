import { useEffect, useRef, useState } from "react";
import { RuneIcon } from "./RuneIcon";
import { RoleIcon } from "./RoleIcon";
import { SpellIcon } from "./SpellIcon";
import { ProBuilds } from "./ProBuilds";
import { RuneEditor } from "./RuneEditor";
import {
  RunesView,
  ProBuild,
  ProBuildsView,
  Selection,
  fmtGames,
  fmtPct,
  selectionFromPreset,
  selectionFromProBuild,
  selectionFromApplied,
  sameSelection,
} from "./types";
import "./runes.css";

type RuneScreen = "presets" | "pro" | "editor";

/** Remembers the open tab so the choice survives a surface switch (for example
 *  the phone swapping to the pick tab) during the same champion select. */
let rememberedScreen: { championId: number; screen: RuneScreen } = {
  championId: -1,
  screen: "presets",
};

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
  onApply: (selection: Selection, presetIndex: number | null, spells: number[] | null) => void;
  onToggleAutoApply: (enabled: boolean) => void;
  onToggleSpellsWithRunes: (enabled: boolean) => void;
  onPickSpell: (slot: "d" | "f", spellId: number) => void;
  /** Loads one page of pros' solo-queue games for the champion and role. */
  onLoadProBuilds: (championId: number, position: string, page: number) => Promise<ProBuildsView>;
  onTierChange: (tier: string) => void;
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

export function RunesPanel({
  mode,
  view,
  loading,
  busy,
  error,
  onApply,
  onToggleAutoApply,
  onToggleSpellsWithRunes,
  onPickSpell,
  onLoadProBuilds,
  onTierChange,
  onExit,
}: Props) {
  const [screen, setScreen] = useState<RuneScreen>("presets");
  const [selection, setSelection] = useState<Selection | null>(null);
  const [proView, setProView] = useState<ProBuildsView | null>(null);
  const [proLoading, setProLoading] = useState(false);
  const [proError, setProError] = useState<string | null>(null);
  const [pickerSlot, setPickerSlot] = useState<"d" | "f" | null>(null);
  const proRequested = useRef(false);

  const championId = view?.championId ?? 0;
  useEffect(() => {
    const restored =
      rememberedScreen.championId === championId ? rememberedScreen.screen : "presets";
    setScreen(restored);
    setSelection(view?.applied ? selectionFromApplied(view.applied) : null);
    setProView(null);
    setProError(null);
    setPickerSlot(null);
    proRequested.current = false;
    // Reset only when the champion changes, not on every statistics refresh.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [championId]);

  useEffect(() => {
    if (championId > 0) rememberedScreen = { championId, screen };
  }, [championId, screen]);

  useEffect(() => {
    if (screen !== "pro" || championId <= 0 || proView || proRequested.current) return;
    proRequested.current = true;
    void loadPro(1);
    // Load once when the tab is opened; "load more" is the only other fetch.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [screen, championId, proView]);

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

  const active = view.applied ? selectionFromApplied(view.applied) : null;
  const dirty = selection !== null && !sameSelection(selection, active);
  const canEdit = selection !== null && view.trees.length > 0;
  const applyIndex = selection && sameSelection(selection, active)
    ? view.applied?.presetIndex ?? null
    : null;

  // A stable non-null alias: TypeScript loses the narrowing above inside the
  // handlers declared below.
  const data = view;

  function startEditor() {
    setScreen("editor");
    setSelection((prev) =>
      prev ?? (data.applied ? selectionFromApplied(data.applied) : defaultSelection(data)),
    );
  }

  async function loadPro(page: number) {
    setProLoading(true);
    setProError(null);
    try {
      const next = await onLoadProBuilds(championId, view?.position ?? "none", page);
      setProView((prev) =>
        page > 1 && prev ? { ...next, matches: [...prev.matches, ...next.matches] } : next,
      );
    } catch (reason) {
      setProError(String(reason));
    } finally {
      setProLoading(false);
    }
  }

  function retryPro() {
    proRequested.current = true;
    setProView(null);
    setProError(null);
    void loadPro(1);
  }

  /** Import an exact pro page through the same apply path a preset uses, then
   *  open the editor so it can be tweaked and applied again. */
  function importProBuild(build: ProBuild) {
    const next = selectionFromProBuild(build);
    setSelection(next);
    setScreen("editor");
    onApply(next, null, build.spells);
  }

  /** The name of a spell for a tooltip; falls back when it is off-mode. */
  function spellName(id: number): string {
    if (id <= 0) return "Empty";
    return data.spells.available.find((spell) => spell.id === id)?.name ?? "Summoner spell";
  }

  const spellSlots = (
    <div className="runes-spells" role="group" aria-label="Summoner spells">
      {(["d", "f"] as const).map((slot) => {
        const id = slot === "d" ? data.spells.spell1Id : data.spells.spell2Id;
        return (
          <button
            key={slot}
            type="button"
            className="rune-spell-slot"
            disabled={busy || data.spells.available.length === 0}
            onClick={() => setPickerSlot(slot)}
            title={spellName(id)}
            aria-label={`Change summoner spell ${slot.toUpperCase()}${id > 0 ? ` (${spellName(id)})` : ""}`}
          >
            <SpellIcon id={id} mode={mode} />
            <span className="rune-spell-key">{slot.toUpperCase()}</span>
          </button>
        );
      })}
    </div>
  );

  const sourceClass = view.source === "opgg" ? "is-live" : view.source === "lcu" ? "is-fallback" : "is-none";
  const roleLabel = view.position && view.position !== "none" ? positionLabels[view.position] : modeLabels[view.mode] ?? view.mode;
  const appliedKeystone = view.applied
    ? view.trees.flatMap((tree) => tree.keystones).find((rune) => rune.id === view.applied?.keystone)
    : undefined;
  const autoAppliedLabel = view.applied?.autoApplied
    ? `${appliedKeystone?.name ?? "Recommended runes"} · ${view.applied.name}`
    : null;
  const showLockHint = screen !== "pro" && view.autoApply && !view.applied && view.phase === "ChampSelect" && !view.locked;

  return (
    <div className={`runes-panel runes-${mode}`}>
      <div className="runes-head">
        <div className="runes-title">
          <p className="runes-champ">
            <RoleIcon role={view.position || view.mode} size={13} mode={mode} />
            {view.championName || "Champion"} · {roleLabel}
          </p>
          <h2>Runes</h2>
        </div>
        <div className="runes-head-right">
          {spellSlots}
          {onExit && mode === "desktop" && (
            <button type="button" className="runes-back" onClick={onExit} aria-label="Back to accounts">
              Accounts
            </button>
          )}
        </div>
      </div>

      <div className="runes-tabs" role="tablist">
        <button type="button" role="tab" aria-selected={screen === "presets"} className={screen === "presets" ? "is-active" : ""} onClick={() => setScreen("presets")}>Presets</button>
        <button type="button" role="tab" aria-selected={screen === "pro"} className={screen === "pro" ? "is-active" : ""} onClick={() => setScreen("pro")}>Pro builds</button>
        <button type="button" role="tab" aria-selected={screen === "editor"} className={screen === "editor" ? "is-active" : ""} onClick={startEditor}>Editor</button>
        {screen !== "pro" && <span className={`runes-source ${sourceClass}`}>{view.sourceLabel}</span>}
      </div>

      {screen !== "pro" && (
        <div className="runes-settings">
          {view.tierSupported && (
            <label className="runes-filter-label">
              <span>Rank</span>
              <select
                className="rune-tier-select"
                value={view.tier}
                disabled={busy}
                aria-label="Rank bracket for rune statistics"
                onChange={(event) => onTierChange(event.target.value)}
              >
                {view.tiers.map((option) => (
                  <option key={option.value} value={option.value}>{option.label}</option>
                ))}
              </select>
            </label>
          )}
          <label className="runes-auto" title="Apply the recommended runes when your champion locks in">
            <input
              type="checkbox"
              role="switch"
              checked={view.autoApply}
              disabled={busy}
              onChange={(event) => onToggleAutoApply(event.target.checked)}
            />
            <span>Auto-apply recommended runes</span>
          </label>
          <label className="runes-auto" title="Also set the recommended summoner spells when applying a page">
            <input
              type="checkbox"
              role="switch"
              checked={view.spells.applyWithRunes}
              disabled={busy}
              onChange={(event) => onToggleSpellsWithRunes(event.target.checked)}
            />
            <span>Apply summoner spells with runes</span>
          </label>
          {view.tierSupported && view.games > 0 && (
            <span className="runes-filter-games">{fmtGames(view.games)} games</span>
          )}
        </div>
      )}

      <div className="runes-body">
        {error && <p className="runes-alert" role="alert">{error}</p>}
        {screen !== "pro" && autoAppliedLabel && (
          <p className="runes-note runes-auto-note">Auto-applied {autoAppliedLabel}</p>
        )}
        {showLockHint && <p className="runes-note runes-auto-note">Applies when you lock in.</p>}
        {screen !== "pro" && view.message && !error && !view.tierEmpty && <p className="runes-note">{view.message}</p>}
        {screen !== "pro" && view.source === "opgg" && <p className="runes-note runes-approx">Win%, pick% and games are approximate, aggregated from op.gg builds.</p>}

        {screen === "pro" ? (
          <ProBuilds
            mode={mode}
            view={proView}
            loading={proLoading}
            error={proError}
            busy={busy}
            active={active}
            onImport={importProBuild}
            onLoadMore={() => void loadPro((proView?.page ?? 1) + 1)}
            onRetry={retryPro}
          />
        ) : screen === "presets" ? (
          <div className="runes-presets">
            {view.tierEmpty ? (
              <div className="runes-tier-empty">
                <p>{view.message ?? `Not enough games at ${view.tierLabel}.`}</p>
                {view.tier !== "all" && (
                  <button type="button" disabled={busy} onClick={() => onTierChange("all")}>
                    Use All ranks
                  </button>
                )}
              </div>
            ) : view.presets.length === 0 ? (
              <p className="runes-note">
                {view.canApply ? "No presets for this role yet." : "No recommendations available. You can still build a page in the Editor."}
              </p>
            ) : null}
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
                    onApply(presetSelection, preset.index, preset.spells);
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
                    {preset.spells.length === 2 && (
                      <>
                        <span className="rune-preset-divider" />
                        {preset.spells.map((id, index) => (
                          <SpellIcon key={`sp${index}`} id={id} mode={mode} className="rune-preset-spell" />
                        ))}
                      </>
                    )}
                  </span>
                </button>
              );
            })}
          </div>
        ) : canEdit && selection ? (
          <RuneEditor
            mode={mode}
            view={view}
            selection={selection}
            busy={busy}
            onChange={setSelection}
          />
        ) : (
          <p className="runes-note">The rune grid is unavailable{view.message ? `: ${view.message}` : "."}</p>
        )}
      </div>

      {screen !== "pro" && (
        <div className="runes-foot">
          <div className="runes-actions">
            <button type="button" className="runes-reset" disabled={busy || !dirty || !selection} onClick={() => setSelection(active ?? defaultSelection(data))}>
              <ResetIcon size={14} /> Reset
            </button>
            <button
              type="button"
              className="runes-apply"
              disabled={busy || !selection || (!view.canApply && !dirty)}
              onClick={() => selection && onApply(selection, applyIndex, null)}
            >
              {busy ? <Spinner /> : <CheckIcon size={15} />} Apply page
            </button>
          </div>
        </div>
      )}

      {pickerSlot && (
        <div className="runes-spell-picker" role="dialog" aria-modal="true" aria-label="Choose a summoner spell">
          <div className="runes-spell-sheet">
            <div className="runes-spell-sheet-head">
              <strong>Summoner spell · {pickerSlot.toUpperCase()}</strong>
              <button type="button" onClick={() => setPickerSlot(null)}>Close</button>
            </div>
            <div className="runes-spell-grid">
              {view.spells.available.map((spell) => {
                const currentId = pickerSlot === "d" ? view.spells.spell1Id : view.spells.spell2Id;
                return (
                  <button
                    key={spell.id}
                    type="button"
                    className={`rune-spell-option ${spell.id === currentId ? "is-active" : ""}`}
                    onClick={() => {
                      onPickSpell(pickerSlot, spell.id);
                      setPickerSlot(null);
                    }}
                    title={spell.name}
                    aria-label={spell.name}
                  >
                    <SpellIcon id={spell.id} mode={mode} />
                    <span>{spell.name}</span>
                  </button>
                );
              })}
            </div>
            {view.spells.available.length === 0 && (
              <p className="runes-note">No summoner spells are available right now.</p>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
