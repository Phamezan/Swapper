import { useEffect, useRef, useState } from "react";
import { RuneIcon } from "./RuneIcon";
import { RoleIcon } from "./RoleIcon";
import { SpellIcon } from "./SpellIcon";
import { PresetBuild } from "./PresetBuild";
import { MatchupBanner, EnemyPicker, BuildChoice } from "./MatchupBanner";
import { ChampionTab } from "./ChampionTab";
import { ProBuilds } from "./ProBuilds";
import { CachedBadge } from "./CachedBadge";
import { RuneEditor } from "./RuneEditor";
import { RankPicker } from "./RankPicker";
import {
  RunesView,
  ProBuild,
  ProBuildsView,
  KeystoneBuildView,
  MatchupView,
  ChampionApi,
  Selection,
  fmtGames,
  fmtPct,
  selectionFromPreset,
  selectionFromProBuild,
  selectionFromApplied,
  sameSelection,
} from "./types";
import "./runes.css";

type RuneScreen = "presets" | "pro" | "editor" | "champion";

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
  onApply: (
    selection: Selection,
    presetIndex: number | null,
    spells: number[] | null,
    enemyChampionId?: number | null,
  ) => void;
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
  /** Loads the lane matchup build against the enemy laner, from lolalytics. */
  onLoadMatchup: (
    championId: number,
    enemyChampionId: number,
    position: string,
    tier: string,
  ) => Promise<MatchupView | null>;
  /** Loaders for the Champion tab. */
  championApi: ChampionApi;
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
  onLoadMatchup,
  championApi,
  onTierChange,
}: Props) {
  const [screen, setScreen] = useState<RuneScreen>("presets");
  const [selection, setSelection] = useState<Selection | null>(null);
  const [proView, setProView] = useState<ProBuildsView | null>(null);
  const [proLoading, setProLoading] = useState(false);
  const [proError, setProError] = useState<string | null>(null);
  const [pickerSlot, setPickerSlot] = useState<"d" | "f" | null>(null);
  const [importNotice, setImportNotice] = useState<string | null>(null);
  const [matchup, setMatchup] = useState<MatchupView | null>(null);
  const [matchupLoading, setMatchupLoading] = useState(false);
  const [chosenEnemy, setChosenEnemy] = useState<{ championId: number; enemyId: number } | null>(null);
  const [buildChoice, setBuildChoice] = useState<BuildChoice>("generic");
  // True once the user clicked a tab themselves; their screen then survives
  // champion changes. Without it the Champion tab is only an automatic default.
  const userPickedTab = useRef(false);
  // Changes whenever champion select starts or ends, so the Champion tab drops
  // its search between sessions.
  const inChampSelect = view?.phase === "ChampSelect";
  const [sessionId, setSessionId] = useState(0);
  const lastInChampSelect = useRef(inChampSelect);
  useEffect(() => {
    if (lastInChampSelect.current === inChampSelect) return;
    lastInChampSelect.current = inChampSelect;
    setSessionId((id) => id + 1);
  }, [inChampSelect]);
  const proRequested = useRef(false);
  const proRequest = useRef(0);

  const championId = view?.championId ?? 0;
  const position = view?.position ?? "";
  useEffect(() => {
    // With no champion yet, the Champion tab is the only useful one.
    if (championId <= 0) {
      userPickedTab.current = false;
      setScreen("champion");
    } else if (!userPickedTab.current) {
      setScreen(rememberedScreen.championId === championId ? rememberedScreen.screen : "presets");
    }
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

  // The player's choice wins while that enemy is still in the draft; otherwise
  // the unambiguous auto-detected laner (or nothing) is the default.
  const enemies = view?.enemies ?? [];
  const enemyId =
    chosenEnemy !== null &&
    chosenEnemy.championId === championId &&
    enemies.some((enemy) => enemy.id === chosenEnemy.enemyId)
      ? chosenEnemy.enemyId
      : view?.enemyChampionId ?? 0;
  // With no enemy chosen the Champion tab opens on the champion you picked.
  const ownChampion =
    view && view.championId > 0 ? { id: view.championId, name: view.championName } : null;
  const tier = view?.tier ?? "";
  useEffect(() => {
    let live = true;
    setMatchup(null);
    setBuildChoice("generic");
    setMatchupLoading(false);
    if (championId > 0 && enemyId > 0) {
      setMatchupLoading(true);
      onLoadMatchup(championId, enemyId, position, tier)
        .then((next) => {
          if (live) setMatchup(next);
        })
        .catch(() => {
          if (live) setMatchup(null);
        })
        .finally(() => {
          if (live) setMatchupLoading(false);
        });
    }
    return () => {
      live = false;
    };
    // The loader is a stable wrapper; only the matchup filter should refetch.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [championId, enemyId, position, tier]);

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
    const tabs: [RuneScreen, string][] = [
      ["presets", "Presets"],
      ["pro", "Pro builds"],
      ["champion", "Champion"],
      ["editor", "Editor"],
    ];
    return (
      <div className={`runes-panel runes-${mode}`}>
        <div className="runes-tabs" role="group" aria-label="Runes sections">
          {tabs.map(([id, label]) => (
            <button
              key={id}
              type="button"
              aria-pressed={screen === id}
              className={screen === id ? "is-active" : ""}
              onClick={() => pickTab(id)}
            >
              {label}
            </button>
          ))}
        </div>
        <div className="runes-body">
          {screen === "champion" ? (
            <ChampionTab
              mode={mode}
              initialTier={view?.tier ?? "emerald_plus"}
              prefillChampion={null}
              prefillPosition=""
              sessionKey={String(sessionId)}
              api={championApi}
              onLoadMatchup={onLoadMatchup}
            />
          ) : (
            <div className="runes-empty">
              <h2>Runes</h2>
              <p>{view?.message ?? "Waiting for champion select."}</p>
            </div>
          )}
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

  function pickTab(next: RuneScreen) {
    userPickedTab.current = true;
    setScreen(next);
  }

  function startEditor() {
    userPickedTab.current = true;
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
  const matchupActive = buildChoice === "matchup" && !matchup?.fallback && !!matchup?.preset;
  const shownPresets = matchupActive && matchup?.preset ? [matchup.preset] : view.presets;
  const showLockHint = screen !== "pro" && screen !== "champion" && view.autoApply && !view.applied && view.phase === "ChampSelect" && !view.locked;

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

      <div className="runes-tabs" role="group" aria-label="Runes sections">
        <button type="button" aria-pressed={screen === "presets"} className={screen === "presets" ? "is-active" : ""} onClick={() => pickTab("presets")}>Presets</button>
        <button type="button" aria-pressed={screen === "pro"} className={screen === "pro" ? "is-active" : ""} onClick={() => pickTab("pro")}>Pro builds</button>
        <button type="button" aria-pressed={screen === "champion"} className={screen === "champion" ? "is-active" : ""} onClick={() => pickTab("champion")}>Champion</button>
        <button type="button" aria-pressed={screen === "editor"} className={screen === "editor" ? "is-active" : ""} onClick={startEditor}>Editor</button>
        {view.source === "lcu" && (
          <span className="runes-source is-fallback" title="op.gg had no data for this champion and role (or could not be reached); showing the League client's own recommendations.">
            League fallback
          </span>
        )}
        {view.stale && <CachedBadge stale updatedAt={view.updatedAt} />}
      </div>

      {view.mode === "ranked" && screen !== "champion" && (
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

      {screen !== "pro" && screen !== "champion" && (
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
        {screen !== "pro" && screen !== "champion" && autoAppliedLabel && (
          <p className="runes-note runes-auto-note">Auto-applied {autoAppliedLabel}</p>
        )}
        {showLockHint && <p className="runes-note runes-auto-note">Applies when you lock in.</p>}
        {screen !== "pro" && screen !== "champion" && view.message && !error && !view.tierEmpty && <p className="runes-note">{view.message}</p>}

        {screen === "champion" ? (
          <ChampionTab
            mode={mode}
            initialTier={view.tier}
            prefillChampion={enemies.find((enemy) => enemy.id === enemyId) ?? ownChampion}
            prefillPosition={view.mode === "ranked" && view.position !== "none" ? view.position : ""}
            sessionKey={String(sessionId)}
            api={championApi}
            onLoadMatchup={onLoadMatchup}
          />
        ) : screen === "pro" ? (
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
            ) : shownPresets.length === 0 ? (
              <p className="runes-note">
                {view.canApply ? "No presets for this role yet." : "No recommendations available. You can still build a page in the Editor."}
              </p>
            ) : null}
            {enemies.length > 0 && view.position !== "none" && (
              <EnemyPicker
                enemies={enemies}
                slots={view.enemySlots}
                selected={enemyId}
                disabled={busy}
                onSelect={(id) => setChosenEnemy({ championId, enemyId: id })}
              />
            )}
            {matchupLoading && <p className="matchup-loading">Loading matchup…</p>}
            {matchup && !view.tierEmpty && (
              <MatchupBanner matchup={matchup} choice={buildChoice} disabled={busy} onChoose={setBuildChoice} />
            )}
            {shownPresets.map((preset) => {
              const presetSelection = selectionFromPreset(preset);
              const isActive = sameSelection(presetSelection, active);
              return (
                <div key={matchupActive ? "matchup" : preset.index} className={`rune-preset ${isActive ? "is-active" : ""}`}>
                  <button
                    type="button"
                    className="rune-preset-main"
                    disabled={busy}
                    onClick={() => {
                      setSelection(presetSelection);
                      setScreen("editor");
                      if (matchupActive && matchup) {
                        onApply(presetSelection, null, preset.spells, matchup.enemyChampionId);
                      } else {
                        onApply(presetSelection, preset.index, preset.spells);
                      }
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
                    preloaded={matchupActive ? matchup?.build : null}
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

      {screen !== "pro" && screen !== "champion" && (
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
