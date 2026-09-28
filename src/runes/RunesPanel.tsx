import { useEffect, useRef, useState } from "react";
import { RuneIcon } from "./RuneIcon";
import { RoleIcon } from "./RoleIcon";
import { SpellIcon } from "./SpellIcon";
import { PresetBuild } from "./PresetBuild";
import { ProBuilds } from "./ProBuilds";
import { CachedBadge } from "./CachedBadge";
import { RuneEditor } from "./RuneEditor";
import { RankPicker } from "./RankPicker";
import {
  RunesView,
  ProBuild,
  ProBuildsView,
  KeystoneBuildView,
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
  onToggleImportItems: (enabled: boolean) => void;
  onPickSpell: (slot: "d" | "f", spellId: number) => void;
  onPositionChange: (position: string) => void;
  onImportItems: (
    championId: number,
    championName: string,
    source: string,
    items: number[],
  ) => Promise<void>;
  /** Loads one page of pros' solo-queue games for the champion and role. */
  onLoadProBuilds: (championId: number, position: string, page: number) => Promise<ProBuildsView>;
  /** Loads the 6-item build for one preset's keystone, from lolalytics. */
  onLoadBuild: (
    championId: number,
    position: string,
    tier: string,
    keystone: number,
  ) => Promise<KeystoneBuildView | null>;
  onTierChange: (tier: string) => void;
};

/** Names the champion and role when there are no pro games, so it reads as
 *  "nobody plays this role" rather than a failed load. */
function proEmptyMessage(championName: string, position: string): string | undefined {
  const role = position && position !== "none" ? positionLabels[position] : undefined;
  if (!championName || !role) return undefined;
  return `No recent pro games for ${championName} as ${role} — try another role.`;
}

const positionLabels: Record<string, string> = {
  top: "Top",
  jungle: "Jungle",
  mid: "Mid",
  adc: "Bot",
  support: "Support",
  none: "No role",
};

const runeRoles = [
  { value: "top", label: "Top" },
  { value: "jungle", label: "Jungle" },
  { value: "mid", label: "Mid" },
  { value: "adc", label: "Bot" },
  { value: "support", label: "Support" },
] as const;

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
  onToggleImportItems,
  onPickSpell,
  onPositionChange,
  onImportItems,
  onLoadProBuilds,
  onLoadBuild,
  onTierChange,
}: Props) {
  const [screen, setScreen] = useState<RuneScreen>("presets");
  const [selection, setSelection] = useState<Selection | null>(null);
  const [proView, setProView] = useState<ProBuildsView | null>(null);
  const [proLoading, setProLoading] = useState(false);
  const [proError, setProError] = useState<string | null>(null);
  const [pickerSlot, setPickerSlot] = useState<"d" | "f" | null>(null);
  const [importNotice, setImportNotice] = useState<string | null>(null);
  const proRequested = useRef(false);
  const proRequest = useRef(0);

  const championId = view?.championId ?? 0;
  const position = view?.position ?? "";
  useEffect(() => {
    const restored =
      rememberedScreen.championId === championId ? rememberedScreen.screen : "presets";
    setScreen(restored);
    setSelection(view?.applied ? selectionFromApplied(view.applied) : null);
    proRequest.current += 1;
    setProLoading(false);
    setProView(null);
    setProError(null);
    setPickerSlot(null);
    setImportNotice(null);
    proRequested.current = false;
    // Reset only when the champion changes, not on every statistics refresh.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [championId]);

  useEffect(() => {
    proRequest.current += 1;
    setProLoading(false);
    setProView(null);
    setProError(null);
    proRequested.current = false;
  }, [position]);

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
    const request = ++proRequest.current;
    const requestedPosition = view?.position ?? "none";
    setProLoading(true);
    setProError(null);
    try {
      const next = await onLoadProBuilds(championId, requestedPosition, page);
      if (request === proRequest.current) {
        setProView((prev) =>
          page > 1 && prev ? { ...next, matches: [...prev.matches, ...next.matches] } : next,
        );
      }
    } catch (reason) {
      if (request === proRequest.current) setProError(String(reason));
    } finally {
      if (request === proRequest.current) setProLoading(false);
    }
  }

  function retryPro() {
    proRequested.current = true;
    setProView(null);
    setProError(null);
    void loadPro(1);
  }

  async function importItemSet(source: string, items: number[]) {
    setImportNotice(null);
    try {
      await onImportItems(championId, view?.championName ?? "Champion", source, items);
      setImportNotice("Item set added to the League shop.");
    } catch {
      // The parent displays the request error beside the rune controls.
    }
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
        </div>
      </div>

      <div className="runes-tabs" role="tablist">
        <button type="button" role="tab" aria-selected={screen === "presets"} className={screen === "presets" ? "is-active" : ""} onClick={() => setScreen("presets")}>Presets</button>
        <button type="button" role="tab" aria-selected={screen === "pro"} className={screen === "pro" ? "is-active" : ""} onClick={() => setScreen("pro")}>Pro builds</button>
        <button type="button" role="tab" aria-selected={screen === "editor"} className={screen === "editor" ? "is-active" : ""} onClick={startEditor}>Editor</button>
        {view.source === "lcu" && (
          <span className="runes-source is-fallback" title="op.gg is unavailable; showing the League client's own recommendations.">
            League fallback
          </span>
        )}
        {view.stale && <CachedBadge stale updatedAt={view.updatedAt} />}
      </div>

      {view.mode === "ranked" && (
        <div className="runes-role-picker" role="group" aria-label="Choose rune role">
          {runeRoles.map((role) => (
            <button
              key={role.value}
              type="button"
              title={role.label}
              aria-label={`${role.label} runes`}
              aria-pressed={view.position === role.value}
              className={view.position === role.value ? "is-active" : ""}
              disabled={busy}
              onClick={() => {
                setImportNotice(null);
                onPositionChange(role.value);
              }}
            >
              <RoleIcon role={role.value} size={17} mode={mode} />
            </button>
          ))}
        </div>
      )}

      {screen !== "pro" && (
        <div className="runes-settings">
          {view.tierSupported && (
            <RankPicker
              tiers={view.tiers}
              value={view.tier}
              games={view.games}
              disabled={busy}
              mode={mode}
              onChange={onTierChange}
            />
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
          <label className="runes-auto" title="Also add the preset's item build to the League shop when applying it; Swapper removes it after the game">
            <input
              type="checkbox"
              role="switch"
              checked={view.importItems}
              disabled={busy}
              onChange={(event) => onToggleImportItems(event.target.checked)}
            />
            <span>Import item build</span>
          </label>
        </div>
      )}

      <div className="runes-body">
        {error && <p className="runes-alert" role="alert">{error}</p>}
        {importNotice && <p className="runes-note runes-import-note" role="status">{importNotice}</p>}
        {screen !== "pro" && autoAppliedLabel && (
          <p className="runes-note runes-auto-note">Auto-applied {autoAppliedLabel}</p>
        )}
        {showLockHint && <p className="runes-note runes-auto-note">Applies when you lock in.</p>}
        {screen !== "pro" && view.message && !error && !view.tierEmpty && <p className="runes-note">{view.message}</p>}

        {screen === "pro" ? (
          <ProBuilds
            mode={mode}
            view={proView}
            emptyMessage={proEmptyMessage(view.championName, position)}
            loading={proLoading}
            error={proError}
            busy={busy}
            active={active}
            onImport={importProBuild}
            onImportItems={(build) => void importItemSet(
              `Pro · ${build.proName || "build"}`,
              build.finalItems.map((item) => item.id),
            )}
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
                <div key={preset.index} className={`rune-preset ${isActive ? "is-active" : ""}`}>
                  <button
                    type="button"
                    className="rune-preset-main"
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
                  <PresetBuild
                    mode={mode}
                    championId={view.championId}
                    position={view.position}
                    tier={view.tier}
                    keystone={preset.keystone}
                    onLoad={onLoadBuild}
                  />
                </div>
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
