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
  ProBuild,
  ProBuildsView,
  PresetBuildView,
  MatchupView,
  Selection,
  LOW_SAMPLE_GAMES,
  fmtExactGames,
  fmtPct,
  isLowSample,
  wrTone,
  selectionFromPreset,
  selectionFromProBuild,
  selectionFromApplied,
  sameSelection,
} from "./types";
import { SwordsIcon, CrowdIcon, TrophyIcon, PencilIcon, CheckIcon, ResetIcon, Spinner } from "./RunesPanelIcons";
import type { RunesPanelProps } from "./RunesPanelProps";
import { proEmptyMessage, positionLabels, runeRoles, modeLabels, defaultSelection } from "./runesPanelModel";
import "./runes.css";

type RuneScreen = "presets" | "pro" | "editor" | "champion";

/** Remembers the open tab so the choice survives a surface switch (for example
 *  the phone swapping to the pick tab) during the same champion select. */
let rememberedScreen: { championId: number; screen: RuneScreen } = {
  championId: -1,
  screen: "presets",
};

export function RunesPanel({
  mode,
  view,
  loading,
  busy,
  error,
  shopNotice,
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
  sessionKey,
}: RunesPanelProps) {
  const [screen, setScreen] = useState<RuneScreen>("presets");
  const [selection, setSelection] = useState<Selection | null>(null);
  const [proView, setProView] = useState<ProBuildsView | null>(null);
  const [proLoading, setProLoading] = useState(false);
  const [proError, setProError] = useState<string | null>(null);
  // False shows the pro list, true the OTP list. Kept when loading more pages.
  const [proOtp, setProOtp] = useState(false);
  const [pickerSlot, setPickerSlot] = useState<"d" | "f" | null>(null);
  const [importNotice, setImportNotice] = useState<string | null>(null);
  const [matchupResult, setMatchupResult] = useState<{ key: string; value: MatchupView | null } | null>(null);
  const [buildResults, setBuildResults] = useState<{ key: string; values: Record<number, PresetBuildView | null> } | null>(null);
  const [presetTab, setPresetTab] = useState<"pick" | "win">("pick");
  const [matchupLoading, setMatchupLoading] = useState(false);
  const [chosenEnemy, setChosenEnemy] = useState<{ championId: number; enemyId: number; session: string } | null>(null);
  const [buildChoice, setBuildChoice] = useState<BuildChoice>("matchup");
  // The backend keeps the last applied page per champion and does not clear it
  // when a new session starts, so a fresh champion select can show a stale
  // "Applied". Suppress the confirmation until an apply actually changes it.
  // ponytail: identical re-apply in a new session stays suppressed; acceptable.
  const [appliedStale, setAppliedStale] = useState(false);
  // True once the user clicked a tab themselves; their screen then survives
  // champion changes. Without it the Champion tab is only an automatic default.
  const userPickedTab = useRef(false);
  const proRequested = useRef(false);
  const proRequest = useRef(0);
  // True when the editor is opened from a preset card, so it can scroll its
  // rune grid into view (the grid sits below the settings otherwise).
  const editorFromPreset = useRef(false);

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
    setProOtp(false);
    setPickerSlot(null);
    setImportNotice(null);
    setPresetTab("pick");
    proRequested.current = false;
    // Reset on a champion or session change, not on a statistics refresh.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [championId, sessionKey]);

  useEffect(() => {
    proRequest.current += 1;
    setProLoading(false);
    setProView(null);
    setProError(null);
    setProOtp(false);
    proRequested.current = false;
  }, [position]);

  // The player's choice wins while that enemy is still in the draft; otherwise
  // the unambiguous auto-detected laner (or nothing) is the default.
  const enemies = view?.enemies ?? [];
  const enemyId =
    chosenEnemy !== null &&
    chosenEnemy.championId === championId &&
    chosenEnemy.session === sessionKey &&
    enemies.some((enemy) => enemy.id === chosenEnemy.enemyId)
      ? chosenEnemy.enemyId
      : view?.enemyChampionId ?? 0;
  // With no enemy chosen the Champion tab opens on the champion you picked.
  const ownChampion =
    view && view.championId > 0 ? { id: view.championId, name: view.championName } : null;
  const tier = view?.tier ?? "";
  const matchupKey = `${sessionKey}:${championId}:${enemyId}:${position}:${tier}`;
  const matchup = matchupResult?.key === matchupKey ? matchupResult.value : null;
  const topKeystone = view?.presets[0]?.keystone;
  // Compare against the most-played preset for each keystone, without capping
  // either list. Ignore changing game counts when deciding whether to reload.
  const presetSelections = (view?.presets ?? []).filter((preset, index, presets) =>
    presets.findIndex((other) => other.keystone === preset.keystone) === index).map(selectionFromPreset);
  const generalKey = `${sessionKey}:${championId}:${position}:${tier}:${JSON.stringify(presetSelections)}`;
  const builds = buildResults?.key === generalKey ? buildResults.values : {};
  const generalBuild = topKeystone !== undefined ? builds[topKeystone] ?? null : null;
  const buildsLoading = presetSelections.some((preset) => builds[preset.keystone] === undefined);
  useEffect(() => {
    let live = true;
    setPresetTab("pick");
    setBuildResults({ key: generalKey, values: {} });
    if (championId > 0) {
      for (const preset of presetSelections) {
        void onLoadBuild(championId, position, tier, preset.keystone, preset)
          .catch(() => null)
          .then((value) => {
            if (live) setBuildResults((previous) => ({
              key: generalKey,
              values: { ...(previous?.key === generalKey ? previous.values : {}), [preset.keystone]: value },
            }));
          });
      }
    }
    return () => { live = false; };
    // The key covers all filters and rune selections; the loader is a wrapper.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [generalKey]);
  useEffect(() => {
    let live = true;
    setMatchupResult(null);
    setBuildChoice("matchup");
    setMatchupLoading(false);
    if (championId > 0 && enemyId > 0) {
      setMatchupLoading(true);
      onLoadMatchup(championId, enemyId, position, tier)
        .then((next) => {
          if (live) setMatchupResult({ key: matchupKey, value: next });
        })
        .catch(() => {
          if (live) setMatchupResult({ key: matchupKey, value: null });
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
  }, [matchupKey]);

  useEffect(() => {
    if (championId > 0) rememberedScreen = { championId, screen };
  }, [championId, screen]);

  useEffect(() => {
    if (screen !== "pro" || championId <= 0 || proView || proRequested.current) return;
    proRequested.current = true;
    void loadPro(1);
    // Load once when the tab is opened; "load more" is the only other fetch.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [screen, championId, proView, proOtp]);

  const appliedKey = view?.applied ? JSON.stringify(view.applied) : "";
  useEffect(() => {
    if (view?.applied) {
      const applied = selectionFromApplied(view.applied);
      setSelection((prev) => prev ?? applied);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [appliedKey]);

  // A new session hides the carried-over confirmation; a changed applied page
  // (a real apply) brings it back. Skip the first render so an already-applied
  // page still shows when the panel opens mid-session.
  const lastSession = useRef(sessionKey);
  useEffect(() => {
    if (lastSession.current === sessionKey) return;
    lastSession.current = sessionKey;
    setAppliedStale(true);
  }, [sessionKey]);
  useEffect(() => {
    setAppliedStale(false);
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
              sessionKey={sessionKey}
              api={championApi}
              onLoadMatchup={onLoadMatchup}
            />
          ) : (
            <div className="runes-empty">
              <h2>Runes</h2>
              <p>{view?.message ?? "Waiting for draft."}</p>
            </div>
          )}
        </div>
      </div>
    );
  }

  const active = view.applied && !appliedStale ? selectionFromApplied(view.applied) : null;
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
    editorFromPreset.current = false;
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
      const next = await onLoadProBuilds(championId, requestedPosition, page, proOtp);
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

  /** Switch between the pro and OTP lists, reloading from page one. */
  function chooseProOtp(otp: boolean) {
    if (otp === proOtp) return;
    proRequest.current += 1;
    setProLoading(false);
    setProView(null);
    setProError(null);
    setProOtp(otp);
    proRequested.current = false;
  }

  async function importItemSet(items: number[]) {
    setImportNotice(null);
    try {
      await onImportItems(championId, view?.championName ?? "Champion", items);
      setImportNotice("Item set added to the League shop.");
    } catch {
      // The parent displays the request error beside the rune controls.
    }
  }

  /** Apply a pro's exact page through the same path a preset uses, then add
   *  their items to the shop (removed after the game like any Swapper set).
   *  Sequential so the two requests never race the shared busy state. */
  async function importProBuild(build: ProBuild) {
    const next = selectionFromProBuild(build);
    setSelection(next);
    editorFromPreset.current = false;
    setScreen("editor");
    await onApply(next, null, build.spells);
    if (build.finalItems.length > 0) {
      await importItemSet(build.finalItems.map((item) => item.id));
    }
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
  const winPresets = presetSelections.flatMap((selection) => {
    const win = builds[selection.keystone]?.highestWin;
    return win ? [win.preset] : [];
  });
  const winActive = !matchupActive && presetTab === "win" && winPresets.length > 0;
  const allWinBuildsMatch = presetSelections.length > 0 && presetSelections.every(
    (selection) => builds[selection.keystone]?.highestWinMatchesPick === true,
  );
  const emptyWinTooltip = buildsLoading ? "Loading highest win-rate builds…"
    : allWinBuildsMatch ? "The highest win-rate builds match the most played ones."
    : "No distinct highest win-rate builds are available for this champion and role.";
  const shownPresets = matchupActive && matchup?.preset ? [matchup.preset] : winActive ? winPresets : view.presets;
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
              <RoleIcon role={role.value} size={20} mode={mode} />
            </button>
          ))}
        </div>
      )}


      <div className="runes-body">
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

        {error && <p className="runes-alert" role="alert">{error}</p>}
        {importNotice && <p className="runes-note runes-import-note" role="status">{importNotice}</p>}
        {shopNotice && (
          <p
            className={`runes-note ${shopNotice.ok ? "runes-import-note" : "runes-alert"}`}
            role={shopNotice.ok ? "status" : "alert"}
          >
            {shopNotice.message}
          </p>
        )}
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
            sessionKey={sessionKey}
            api={championApi}
            onLoadMatchup={onLoadMatchup}
          />
        ) : screen === "pro" ? (
          <ProBuilds
            mode={mode}
            view={proView}
            emptyMessage={proEmptyMessage(view.championName, position, proOtp)}
            loading={proLoading}
            error={proError}
            busy={busy}
            active={active}
            otp={proOtp}
            onOtpChange={chooseProOtp}
            onImport={(build) => void importProBuild(build)}
            onLoadMore={() => void loadPro((proView?.page ?? 1) + 1)}
            onRetry={retryPro}
          />
        ) : screen === "presets" ? (
          <div className="runes-presets">
            {!matchupActive && !view.tierEmpty && view.presets.length > 0 && (
              <div className="runes-tabs preset-build-tabs" role="group" aria-label="Preset builds">
                <button type="button" aria-pressed={!winActive} className={!winActive ? "is-active" : ""}
                  onClick={() => setPresetTab("pick")}><CrowdIcon />Most played <em>{view.presets.length}</em></button>
                <button type="button" aria-pressed={winActive} className={winActive ? "is-active" : ""}
                  aria-disabled={winPresets.length === 0}
                  title={winPresets.length === 0 ? emptyWinTooltip : undefined}
                  onClick={() => { if (winPresets.length > 0) setPresetTab("win"); }}>
                  <TrophyIcon />Highest win rate <em>{winPresets.length}</em>
                </button>
              </div>
            )}
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
                {view.canApply ? "No presets for this role yet." : "No recommendations. Build a page in the Editor."}
              </p>
            ) : null}
            {view.enemySlots > 0 && view.position !== "none" && (
              <EnemyPicker
                mode={mode}
                laner={view.enemyChampionId}
                enemies={enemies}
                slots={view.enemySlots}
                selected={enemyId}
                disabled={busy}
                onSelect={(id) => { setChosenEnemy({ championId, enemyId: id, session: sessionKey }); setBuildChoice("matchup"); }}
              />
            )}
            {matchupLoading && <p className="matchup-loading">Loading matchup…</p>}
            {matchup && !view.tierEmpty && (
              <MatchupBanner mode={mode} matchup={matchup} championId={championId} championName={view.championName}
                lane={roleLabel} rank={view.tierLabel} general={generalBuild} preset={view.presets[0]} spells={view.spells.available} />
            )}
            {enemyId > 0 && !matchupLoading && !matchup && <p className="matchup-waiting">No {roleLabel.toLowerCase()} data vs {enemies.find((enemy) => enemy.id === enemyId)?.name ?? view.enemyChampionName}. Showing the general build.</p>}
            {shownPresets.map((preset) => {
              const win = winActive ? builds[preset.keystone]?.highestWin : null;
              const presetSelection = selectionFromPreset(preset);
              const isActive = sameSelection(presetSelection, active);
              const isPending = busy && !isActive && selection !== null && sameSelection(presetSelection, selection);
              const keystoneName = view.trees.flatMap((tree) => tree.keystones).find((rune) => rune.id === preset.keystone)?.name ?? preset.title;
              const build = matchupActive ? matchup?.build ?? null : win ? win.build : builds[preset.keystone] ?? null;
              const loadingBuild = !matchupActive && builds[preset.keystone] === undefined;
              // Lolalytics core numbers of the shown build; op.gg's only once no build is coming.
              const fromBuild = !!build && build.coreGames > 0;
              const games = fromBuild ? build.coreGames : loadingBuild && buildsLoading ? null : preset.play;
              const winPct = fromBuild ? build.coreWinPct : loadingBuild && buildsLoading ? null : preset.winPct;
              const lowSample = games !== null && isLowSample(games);
              const canClick = !busy && (isActive || view.canApply);
              // A second click on the applied card opens it in the Editor instead of re-applying.
              const applyPreset = () => {
                if (!canClick) return;
                setSelection(presetSelection);
                if (isActive) {
                  userPickedTab.current = true;
                  editorFromPreset.current = true;
                  setScreen("editor");
                  return;
                }
                if (matchupActive && matchup) {
                  void onApply(presetSelection, null, preset.spells, matchup.enemyChampionId);
                } else if (winActive) {
                  void onApply(presetSelection, null, preset.spells, null, true);
                } else {
                  void onApply(presetSelection, preset.index, preset.spells);
                }
              };
              const presetTitle = matchupActive ? `Build vs ${matchup?.enemyName}` : winActive ? keystoneName : preset.title;
              return (
                <div
                  key={matchupActive ? matchupKey : `${generalKey}:${winActive ? `win:${preset.keystone}` : preset.index}`}
                  className={`rune-preset ${matchupActive ? "is-tailored" : ""} ${winActive ? "is-highest-win" : ""} ${isActive ? "is-active" : ""} ${isPending ? "is-pending" : ""}`}
                  role="button"
                  tabIndex={0}
                  aria-label={isActive ? `Edit ${presetTitle} in the Editor` : `Apply ${presetTitle} build`}
                  aria-disabled={!canClick}
                  onClick={applyPreset}
                  onKeyDown={(event) => {
                    // Only the card itself: Enter/Space on a nested control must not apply.
                    if (event.target !== event.currentTarget || (event.key !== "Enter" && event.key !== " ")) return;
                    event.preventDefault();
                    applyPreset();
                  }}
                >
                  <div className="rune-preset-head">
                    <span className="rune-preset-copy">
                      <strong>{presetTitle}</strong>
                      <small className="rune-preset-games">
                        <SwordsIcon />
                        {games === null ? "–" : games > 0 ? fmtExactGames(games) : "–"}
                        {lowSample && <span className="rune-low" title={`Under ${LOW_SAMPLE_GAMES} games`}>small sample</span>}
                      </small>
                    </span>
                    {winPct !== null && winPct > 0 && <b className={`rune-preset-wr wr-${wrTone(winPct)}`}>{fmtPct(winPct)}</b>}
                  </div>
                  <div className="rune-preset-icons" aria-label="Runes and summoner spells">
                    <RuneIcon id={preset.keystone} mode={mode} className="rune-preset-keystone" />
                    {preset.primaryRunes.map((id) => <RuneIcon key={`p${id}`} id={id} mode={mode} />)}
                    <span className="rune-preset-divider" />
                    {preset.secondaryRunes.map((id) => <RuneIcon key={`s${id}`} id={id} mode={mode} />)}
                    <span className="rune-preset-divider" />
                    {preset.shards.map((id, index) => <RuneIcon key={`m${index}`} id={id} mode={mode} className="rune-preset-shard" />)}
                    {preset.spells.length === 2 && <span className="preset-spell-box" aria-label="Summoner spells">
                      {preset.spells.map((id, index) => <SpellIcon key={index} id={id} mode={mode} className="rune-preset-spell" />)}
                    </span>}
                  </div>
                  <PresetBuild
                    mode={mode}
                    build={build}
                    loading={loadingBuild}
                    matchupOnly={matchupActive}
                  />
                  {matchup && !matchup.fallback && matchup.preset && <button
                    type="button"
                    className="matchup-build-link"
                    disabled={busy}
                    onClick={(event) => { event.stopPropagation(); setBuildChoice(matchupActive ? "generic" : "matchup"); }}
                    onKeyDown={(event) => event.stopPropagation()}
                  >
                    {matchupActive ? "Use the general build instead" : `Use the build for ${matchup.enemyName}`}
                  </button>}
                  {isActive ? <>
                    <span className="corner-badge"><CheckIcon size={12} /> Applied</span>
                    <span className="rune-preset-edit" aria-hidden><PencilIcon /><i>Edit</i></span>
                  </> : isPending && <span className="corner-badge is-pending">Applying…</span>}
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
            focusGrid={editorFromPreset.current}
            onChange={setSelection}
          />
        ) : (
          <p className="runes-note">The rune grid is unavailable{view.message ? `: ${view.message}` : "."}</p>
        )}
      </div>

      {screen === "editor" && (
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
