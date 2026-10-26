import { Fragment, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ArrowRight, ChevronRight, Trash2 } from "lucide-react";
import { CheckIcon } from "./ui/icons";
import { ChampionIcon } from "../runes/ChampionIcon";
import { ItemIcon } from "../runes/ItemIcon";
import { RuneIcon } from "../runes/RuneIcon";
import { TeamLogo } from "../runes/TeamLogo";
import { SpellIcon } from "../runes/SpellIcon";
import { CachedBadge } from "../runes/CachedBadge";
import { Switch } from "../components/ui/switch";
import { fmtPct, selectionFromPreset, shopNoticeFrom, togglePrimary, toggleSecondary, selectShard, shardChosen } from "../runes/types";
import type { ChampionBuild, ItemsImported, MatchupView, PresetBuildView, ProBuild, ProBuildsView, RuneRow, RuneTree, RunesView, Selection } from "../runes/types";
import { overviewSelection, spellLayout, type FlashKey } from "./buildLogic";
import { applyStatus, itemSetKey, maxOrder, skillGrid, type ApplyStatus } from "./champ-select-model";
import { NearViewport } from "./NearViewport";
import { AbilityHover } from "./AbilityHover";
import { Picker } from "./Picker";
import { isBoots, mergeProPage, mixPro, proVariant, type ProPart, type BuildVariant as Variant } from "./champ-select-pro";
import { Slot } from "./ui/Slot";
import { Notice, Skeleton } from "./ui/States";
import { fmtCount } from "./ui/format";
import "./champ-select.css";
import "./build-panel.css";

export type RuneCatalog = { trees: RuneTree[]; shards: RuneRow[] };
type Ability = { key: string; name: string; icon: string | null };
type Section = keyof ApplyStatus;
/** A rune page the user saved for this champion. */
type SavedRunes = { name: string; selection: Selection };
const sameRunes = (a: Selection, b: Selection) => a.primaryPageId === b.primaryPageId && a.secondaryPageId === b.secondaryPageId && a.keystone === b.keystone
  && [a.primaryRunes, a.secondaryRunes, a.shards].every((ids, i) => ids.join() === [b.primaryRunes, b.secondaryRunes, b.shards][i].join());
const PRO_STEP = 6;
/** The auto-apply setting, shown inline on the champion select page. */
export type AutoApply = { checked: boolean; disabled: boolean; onChange: (enabled: boolean) => void };

export function BuildPanel({ championId, name, lane, tier, build, liveView, matchups = [], preferMatchup, autoApply, preview, proPageSize, loading }: {
  championId: number; name: string; lane: string; tier: string; build: ChampionBuild | null; liveView?: RunesView | null;
  /** The lane matchup, or each possible one; adds a "vs <enemy>" tab for each with its own build. */
  matchups?: MatchupView[]; autoApply?: AutoApply;
  /** Browsing only (champion pages, Matchups): no apply controls and no rune editing; champion select applies. */
  preview?: boolean;
  /** Draft pages through pro builds to fit the window without a scrolling list. */
  proPageSize?: number;
  /** Open on the matchup build instead of the most played one, when there is one. */
  preferMatchup?: boolean;
  /** The build is still being fetched: show a skeleton instead of "unavailable". */
  loading?: boolean;
}) {
  const [catalog, setCatalog] = useState<RuneCatalog | null>(null);
  const [details, setDetails] = useState<PresetBuildView | null>(null);
  const [abilities, setAbilities] = useState<Ability[]>([]);
  // Label of the clicked tab; null until then, so the default follows what loaded. Labels are unique, indexes shift as tabs load.
  const [chosenLabel, setChosenLabel] = useState<string | null>(null);
  const [client, setClient] = useState<RunesView | null>(liveView ?? null);
  const [busy, setBusy] = useState<Section | "all" | null>(null);
  const [notice, setNotice] = useState("");
  // The last apply that failed: which section and the reason the client or Swapper gave.
  const [failed, setFailed] = useState<{ section: Section | "all"; message: string } | null>(null);
  const [imported, setImported] = useState<string | null>(null);
  const [proLoading, setProLoading] = useState(false);
  const proRequest = useRef(0);
  const proBusy = useRef(false);
  const [pros, setPros] = useState<ProBuildsView | null>(null);
  const [pro, setPro] = useState<ProBuild | null>(null);
  const [proPart, setProPart] = useState<ProPart>("all");
  // Pages hold many matches; show a few at a time so the build column stays aligned.
  const [proLimit, setProLimit] = useState(PRO_STEP);
  const [proOffset, setProOffset] = useState(0);
  const [proError, setProError] = useState("");
  const [flashKey, setFlashKey] = useState<FlashKey>("keep");
  useEffect(() => {
    let live = true;
    const load = () => void invoke<FlashKey>("get_flash_key").then(key => { if (live) setFlashKey(key); }).catch(() => {});
    load();
    const stop = listen("runes_changed", load);
    return () => { live = false; void stop.then(unlisten => unlisten()); };
  }, []);
  const [custom, setCustom] = useState<Selection | null>(null);
  const [presets, setPresets] = useState<SavedRunes[]>([]);
  const [presetName, setPresetName] = useState<string | null>(null);
  // The name being typed into the save box; null while it is closed.
  const [naming, setNaming] = useState<string | null>(null);
  const [listTab, setListTab] = useState<"builds" | "presets">("builds");
  useEffect(() => {
    let live = true;
    setPresets([]); setPresetName(null); setNaming(null);
    if (!preview) void invoke<SavedRunes[]>("get_rune_presets", { championId }).then(value => { if (live) setPresets(value); }).catch(() => {});
    return () => { live = false; };
  }, [championId, preview]);
  useEffect(() => {
    let live = true;
    setImported(null);
    void invoke<RuneCatalog>("full_app_rune_catalog").then(value => { if (live) setCatalog(value); }).catch(() => {});
    void invoke<Ability[]>("full_app_abilities", { championId }).then(value => { if (live) setAbilities(value); }).catch(() => {});
    const update = () => invoke<RunesView>("get_runes").then(value => { if (live) setClient(value); }).catch(() => {});
    if (!liveView) void update();
    const events = listen("champ_select", () => { void update(); });
    const items = listen<ItemsImported>("items_imported", ({ payload }) => { if (live && !payload.ok) setNotice(shopNoticeFrom(payload).message); });
    return () => { live = false; void events.then(stop => stop()); void items.then(stop => stop()); };
  }, [championId]);
  useEffect(() => { if (liveView) setClient(liveView); }, [liveView]);
  const selection = overviewSelection(build, catalog?.trees ?? []);
  const sourceKey = JSON.stringify(build);
  useEffect(() => {
    setDetails(null); setChosenLabel(null); setPro(null); setPros(null); setProLimit(PRO_STEP); setProOffset(0); setNotice(""); setFailed(null); setCustom(null);
    proRequest.current++; proBusy.current = false; setProLoading(false); setProError("");
    setImported(null);
  }, [championId, lane, tier, sourceKey]);
  useEffect(() => {
    let live = true;
    if (build?.runes) void invoke<PresetBuildView | null>("get_keystone_build", { championId, position: lane, tier, keystone: build.runes.keystone, mostPlayed: selection })
      .then(value => { if (live) setDetails(value); }).catch(() => {});
    return () => { live = false; };
  }, [championId, lane, tier, sourceKey, catalog]);
  useEffect(() => { void loadPros(1); }, [championId, lane, tier, sourceKey]);
  useEffect(() => () => { proRequest.current++; }, []);
  const loadPros = async (page: number) => {
    if (proBusy.current) return false;
    const request = ++proRequest.current;
    proBusy.current = true; setProLoading(true);
    setProError("");
    try {
      const value = await invoke<ProBuildsView>("get_pro_builds", { championId, position: lane, page, isOtp: false });
      if (request !== proRequest.current) return false;
      if (value.unavailable) throw new Error(value.message || "Pro builds unavailable. Try again.");
      setPros(previous => mergeProPage(page === 1 ? null : previous, value));
      return true;
    } catch (error) { if (request === proRequest.current) setProError(String(error)); return false; }
    finally { if (request === proRequest.current) { proBusy.current = false; setProLoading(false); } }
  };

  if (!build && loading) return <Skeleton label="Loading build" rows={4} height={64} />;
  if (!build) return <section className="fa-card fa-build-empty"><h3>Build unavailable</h3><p>Try another role or rank.</p></section>;
  const variants: Variant[] = [{ label: "Most played", selection, items: details ?? build.items, spells: build.spells, order: build.skillOrder, winPct: (details ?? build.items).coreWinPct, games: (details ?? build.items).coreGames }];
  if (details?.highestWin && !details.highestWinMatchesPick) {
    const high = details.highestWin;
    variants.push({ label: "Highest WR", selection: selectionFromPreset(high.preset), items: high.build, spells: high.preset.spells, order: high.build.skillOrder ?? null, winPct: high.build.coreWinPct, games: high.build.coreGames });
  }
  for (const matchup of matchups) {
    if (matchup.preset && matchup.build && !matchup.fallback) variants.push({ label: `vs ${matchup.enemyName}`, enemyId: matchup.enemyChampionId, selection: selectionFromPreset(matchup.preset), items: matchup.build, spells: matchup.preset.spells, order: matchup.build.skillOrder ?? null, winPct: matchup.stats?.winPct ?? matchup.build.coreWinPct, games: matchup.stats?.games ?? matchup.build.coreGames });
  }
  const matchupIndex = variants.findIndex(variant => variant.label.startsWith("vs "));
  const labelIndex = chosenLabel === null ? -1 : variants.findIndex(variant => variant.label === chosenLabel);
  const variantIndex = labelIndex >= 0 ? labelIndex : preferMatchup && matchupIndex > 0 ? matchupIndex : 0;
  const base = variants[variantIndex] ?? variants[0];
  const chosen: Variant = pro ? mixPro(base, proVariant(pro), proPart) : base;
  const chosenSelection = custom ?? chosen.selection;
  // The saved preset being shown, while its runes are unchanged.
  const shownPreset = custom && presets.find(preset => preset.name === presetName && sameRunes(preset.selection, custom));
  const complete = !!chosenSelection && chosenSelection.primaryRunes.length === 3 && chosenSelection.secondaryRunes.length === 2 && chosenSelection.shards.length === 3;
  const inSelect = client?.phase === "ChampSelect" && client.championId === championId;
  const itemIds = chosen.items.items.map(item => item.id);
  const itemsKey = itemSetKey(championId, itemIds);
  const chosenSpells = spellLayout(chosen.spells, [client?.spells.spell1Id ?? 0, client?.spells.spell2Id ?? 0], flashKey);
  const status = applyStatus(client, championId, complete ? chosenSelection : null, chosenSpells, itemsKey, imported);
  const ready: ApplyStatus = { runes: inSelect && complete, spells: inSelect && chosen.spells.length === 2, items: itemIds.length > 0 };
  const boots = chosen.items.items.filter(isBoots);
  const later = chosen.items.items.slice(chosen.items.coreLen || 3).filter(item => !isBoots(item));
  const order = maxOrder(chosen.order, chosen.items.skillPriority);
  const showTab = (label: string) => { setPro(null); setChosenLabel(label); setCustom(null); setNotice(""); setFailed(null); };
  const savePreset = async (name: string) => {
    if (!custom) return;
    try { setPresets(await invoke<SavedRunes[]>("save_rune_preset", { championId, name, selection: custom })); setPresetName(name.trim()); setNaming(null); setNotice(""); }
    catch (error) { setNotice(String(error).replace(/^Error: /, "")); }
  };
  const deletePreset = async (name: string) => {
    try { setPresets(await invoke<SavedRunes[]>("delete_rune_preset", { championId, name })); }
    catch (error) { setNotice(String(error).replace(/^Error: /, "")); }
  };

  const apply = async (section: Section | "all") => {
    setBusy(section); setNotice(""); setFailed(null);
    try {
      const runes = section === "runes" || (section === "all" && ready.runes && !status.runes);
      const spells = section === "spells" || (section === "all" && ready.spells && !status.spells);
      const items = section === "items" || (section === "all" && ready.items && !status.items);
      if (runes || spells) {
        const current = await invoke<RunesView>("get_runes");
        setClient(current);
        if (current.phase !== "ChampSelect" || current.championId !== championId) throw new Error(`Select ${name} in League Client before applying this build.`);
      }
      if (runes) await invoke("apply_rune_page", { selection: chosenSelection, presetIndex: null, spells: null, position: lane, enemyChampionId: null, highestWin: false });
      if (spells) for (const [index, slot] of (["d", "f"] as const).entries()) await invoke("apply_spell", { slot, spellId: chosenSpells[index] });
      if (items) { await invoke("import_full_item_build", { championId, championName: name, build: chosen.items }); setImported(itemsKey); }
      if (runes || spells) setClient(await invoke<RunesView>("get_runes"));
    } catch (error) { setFailed({ section, message: String(error).replace(/^Error: /, "") }); }
    finally { setBusy(null); }
  };
  const allDone = (!ready.runes || status.runes) && (!ready.spells || status.spells) && (!ready.items || status.items);
  const sections: [Section, string][] = [["runes", "Runes"], ["spells", "Spells"], ["items", "Item set"]];

  return <div className="fa-build-section cs-build-browser">
    <aside className="cs-build-list" aria-label="Browse builds">
      {!preview && <div className="cs-list-tabs" role="group" aria-label="Build list">
        <button aria-pressed={listTab === "builds"} onClick={() => setListTab("builds")}>Builds</button>
        <button aria-pressed={listTab === "presets"} onClick={() => setListTab("presets")}>My presets{presets.length > 0 && <small>{presets.length}</small>}</button>
      </div>}
      {/* Each row is followed by its delete button: the list's grid puts them side by side. */}
      {listTab === "presets" && !preview ? presets.length ? <div className="cs-build-entries is-presets">{presets.map(preset => <Fragment key={preset.name}><button className="cs-build-row" disabled={!!busy} aria-pressed={shownPreset?.name === preset.name} onClick={() => { setPro(null); setCustom(preset.selection); setPresetName(preset.name); setNotice(""); setFailed(null); }}>
        <span className="cs-row-text"><strong>{preset.name}</strong><small>Rune page</small></span>
        <BuildIcons keystone={preset.selection.keystone} secondary={preset.selection.secondaryPageId} items={[]} />
      </button>
      <button className="cs-preset-delete" disabled={!!busy} aria-label={`Delete preset ${preset.name}`} title="Delete preset" onClick={() => { if (shownPreset?.name === preset.name) setCustom(null); void deletePreset(preset.name); }}><Trash2 size={15} aria-hidden /></button></Fragment>)}</div> :<p>No presets for {name} yet. Change a rune, then choose Save as preset.</p> : <>
      {preview && <h3>Builds</h3>}<div className="cs-build-entries">{variants.map((variant, index) => <button className={`cs-build-row${variant.enemyId ? " is-matchup" : ""}`} disabled={!!busy} key={variant.label} aria-pressed={!pro && !shownPreset && index === variantIndex} onClick={() => showTab(variant.label)}>
        {variant.enemyId ? <span className="cs-row-logo"><ChampionIcon id={variant.enemyId} name={variant.label.slice(3)} mode="desktop" /></span> : null}
        <span className="cs-row-text"><strong>{variant.label}</strong><small>{fmtCount(variant.games)} games</small></span>
        <BuildIcons keystone={variant.selection?.keystone} secondary={variant.selection?.secondaryPageId} items={variant.items.items} />
        <b className="cs-row-chip">{fmtPct(variant.winPct)}</b>
      </button>)}</div>
      <h3>Pro builds <small>Recent matches</small></h3>
      <div className="cs-build-entries">{pros?.matches.slice(proOffset, proOffset + (proPageSize ?? proLimit)).map(match => <button className="cs-build-row is-pro" disabled={!!busy} key={match.matchId} aria-pressed={pro?.matchId === match.matchId} onClick={() => { setPro(match); setProPart("all"); setCustom(null); setNotice(""); setFailed(null); }}
        title={`${[match.team, match.league].filter(Boolean).join(" · ")} · ${match.kills}/${match.deaths}/${match.assists}`}>
        <TeamLogo team={match.team} size={30} mode="desktop" className="cs-row-logo" />
        <span className="cs-row-text"><strong>{match.proName}</strong><small>{match.playedAgo}</small></span>
        <BuildIcons keystone={match.keystone} secondary={match.secondaryPageId} items={proVariant(match).items.items} />
        <b className={`cs-row-chip ${match.win ? "up" : "down"}`}>{match.win ? "W" : "L"}</b>
      </button>)}</div>
      {pros?.stale && <CachedBadge stale updatedAt={pros.updatedAt} />}
      {proError && <p role="alert">{proError}</p>}
      {proLoading && (!pros || !proPageSize) ? <Skeleton label="Loading pro builds" rows={proPageSize ?? 4} height={46} /> : proError ? <button className="fa-text-button" onClick={() => void loadPros((pros?.page ?? 0) + 1)}>Retry</button>
        : pros && !pros.matches.length ? <p>No recent pro builds for this role.</p> : proPageSize && pros ? <nav className="cs-pro-pages" aria-label="Pro build pages">
          <button className="fa-text-button" disabled={proOffset === 0 || proLoading} onClick={() => setProOffset(offset => Math.max(0, offset - proPageSize))}>Previous</button>
          <span>{Math.floor(proOffset / proPageSize) + 1}</span>
          <button className="fa-text-button" disabled={proLoading || (proOffset + proPageSize >= pros.matches.length && !pros.hasMore)} onClick={async () => {
            const next = proOffset + proPageSize;
            if (next + proPageSize > pros.matches.length && pros.hasMore && !await loadPros(pros.page + 1)) return;
            setProOffset(next);
          }}>{proLoading ? "Loading…" : "Next"}</button>
        </nav> : pros && (pros.matches.length > proLimit || pros.hasMore) ? <button className="fa-text-button cs-load-pros" onClick={() => { setProLimit(limit => limit + PRO_STEP); if (pros.matches.length <= proLimit + PRO_STEP && pros.hasMore) void loadPros(pros.page + 1); }}>Load more</button> : null}
      </>}
    </aside>
    <div className="cs-build-content">
    <div className="cs-build-selected"><strong>{pro ? `${pro.proName}’s ${proPart === "runes" ? "runes" : proPart === "build" ? "items" : "build"}` : chosen.label}</strong><small>{pro ? proPart === "all" ? `${pro.team} · ${pro.playedAgo} · Patch ${pro.patch}` : `with ${base.label} ${proPart === "runes" ? "items" : "runes"}` : `${name} · ${lane}`}</small>
      {pro && <div className="fa-seg cs-pro-part" role="group" aria-label="Use from this pro build"><small>Use</small>{([["all", "Everything"], ["runes", "Runes only"], ["build", "Items only"]] as const).map(([value, label]) => <button key={value} aria-pressed={proPart === value} disabled={!!busy} onClick={() => { setProPart(value); setCustom(null); }}>{label}</button>)}</div>}</div>
    {!preview && <><div className="cs-apply-bar" aria-label="Apply to League Client">
      {autoApply && <label className="cs-auto-apply">Auto-apply<Switch aria-label="Auto-apply runes" checked={autoApply.checked} disabled={autoApply.disabled} onCheckedChange={autoApply.onChange} /></label>}
      <button className="fa-primary" disabled={!!busy || allDone || !(ready.runes || ready.spells || ready.items)} onClick={() => void apply("all")}><CheckIcon size={16} />{busy === "all" ? "Applying…" : allDone ? "All applied" : "Apply all"}</button>
    </div>
    {failed && <Notice tone="error" action={<button className="fa-secondary" disabled={!!busy} onClick={() => void apply(failed.section)}>Try again</button>}><b>{failed.section === "all" ? "Couldn’t apply the build." : `Couldn’t apply ${sections.find(([key]) => key === failed.section)?.[1].toLowerCase()}.`}</b> {failed.message}</Notice>}
    {notice && <Notice>{notice}</Notice>}</>}
    <section className="fa-card fa-build-grid cs-build-grid">
      <div className="fa-rune-area"><div className="fa-section-heading"><h4>{shownPreset ? shownPreset.name : custom ? "Custom runes" : "Runes"}</h4>{preview ? null : custom ? <span className="cs-rune-actions">
        {!shownPreset && naming === null && presets.some(preset => preset.name === presetName) && <button className="fa-text-button" disabled={!!busy} onClick={() => void savePreset(presetName!)}>Update {presetName}</button>}
        {!shownPreset && naming === null && <button className="fa-text-button" disabled={!!busy} onClick={() => setNaming("")}>Save as preset</button>}
        <button className="fa-text-button" disabled={!!busy} onClick={() => { setCustom(null); setNaming(null); }}>Reset</button></span> : <small className="fa-rune-hint">Click a rune to change it</small>}</div>
        {naming !== null && custom && <form className="cs-preset-save" onSubmit={event => { event.preventDefault(); void savePreset(naming); }}>
          <input autoFocus value={naming} maxLength={40} onChange={event => setNaming(event.target.value)} placeholder={`Name this ${name} rune page`} aria-label="Preset name" />
          <button className="fa-secondary" type="submit" disabled={!naming.trim()}>Save</button>
          <button className="fa-text-button" type="button" onClick={() => setNaming(null)}>Cancel</button>
        </form>}{catalog && chosenSelection ? <RunePage catalog={catalog} selection={chosenSelection} onChange={busy || preview ? undefined : setCustom} /> : <div className="fa-rune-fallback"><p>Rune catalog unavailable</p>{chosenSelection && [chosenSelection.keystone, ...chosenSelection.primaryRunes, ...chosenSelection.secondaryRunes, ...chosenSelection.shards].map((id, index) => <RuneIcon key={index} id={id} mode="desktop" />)}</div>}</div>
      <div className="fa-spell-area cs-skill-area"><div><h4>Summoners</h4><div className="fa-spells">{chosenSpells.map((id, index) => <span key={index}><SpellIcon id={id} mode="desktop" /><kbd>{index ? "F" : "D"}</kbd></span>)}</div></div>
        <div><h4>Skill order</h4>
        {order.length ? <div className="fa-skill-priority" aria-label={`Max order ${order.join(", ")}`}>{order.map((key, index) => <span key={key}>{index > 0 && <ChevronRight size={14} />}<AbilityBadge championId={championId} ability={abilities.find(ability => ability.key === key)} skill={key} /></span>)}</div> : <p>Max order unavailable</p>}
        {chosen.order ? <SkillOrderGrid order={chosen.order} abilities={abilities} /> : <p>Level order unavailable</p>}</div>
      </div>
      {/* Items run in buy order as one full-width row. */}
      <div className="fa-item-area cs-items-row"><h4>Item build</h4><div className="cs-items">
        <section><h5>Starting</h5><div className="fa-item-line">{chosen.items.starters.map(item => <span className="fa-item-stack" key={item.id}><Slot kind="item" id={item.id} label={item.name} size={40} />{item.count > 1 && <b>×{item.count}</b>}</span>)}{!chosen.items.starters.length && <small>Not reported</small>}</div></section>
        {boots.length > 0 && <section><h5>Boots</h5><div className="fa-item-line">{boots.map(item => <Slot kind="item" key={item.id} id={item.id} label={item.name} size={40} />)}</div></section>}
        <section><h5>Core path {pro ? <small>Played in this match</small> : chosen.items.coreGames > 0 && <small>{fmtPct(chosen.items.coreWinPct)} · {fmtCount(chosen.items.coreGames)} games</small>}</h5><div className="fa-item-path">{chosen.items.items.slice(0, chosen.items.coreLen || 3).filter(item => !boots.includes(item)).map((item, index) => <span key={`${item.id}-${index}`}>{index > 0 && <ArrowRight size={14} />}<Slot kind="item" id={item.id} label={item.name} size={40} /></span>)}</div></section>
        {later.length > 0 && <section><h5>Later <small>{pro ? "Completed in this match" : "Popular for each slot"}</small></h5><div className="fa-item-line">{later.map(item => <Slot kind="item" key={item.id} id={item.id} label={item.name} size={40} />)}</div></section>}
        {chosen.items.options.length > 0 && <section><h5>Situational</h5><div className="fa-item-line">{chosen.items.options.slice(0, 7).map(item => <span key={`${item.id}-${item.slot}`} title={`${item.name} · slot ${item.slot} · ${fmtPct(item.winPct)} · ${fmtCount(item.games)} games`}><Slot kind="item" id={item.id} size={40} /></span>)}</div></section>}
      </div>{chosen.items.stale && <CachedBadge stale updatedAt={chosen.items.updatedAt} />}</div>
    </section>
    </div>
  </div>;
}

function AbilityBadge({ championId, ability, skill }: { championId: number; ability?: Ability; skill: string }) {
  return <AbilityHover championId={championId} abilityKey={skill} icon={ability?.icon}><span className="fa-ability">{ability?.icon && <img src={ability.icon} alt={ability.name} loading="lazy" />}<kbd>{skill}</kbd></span></AbilityHover>;
}

function SkillOrderGrid({ order, abilities }: { order: string; abilities: Ability[] }) {
  return <div className="cs-skill-grid" role="table" aria-label={`Skill order by level: ${order.split("").join(" ")}`}>
    {skillGrid(order).map(row => <div className="cs-skill-row" role="row" key={row.skill}>
      <span role="rowheader" className={row.skill === "R" ? "is-ultimate" : ""} title={abilities.find(ability => ability.key === row.skill)?.name}>{row.skill}</span>
      {row.cells.map((level, index) => <b role="cell" key={index} className={level ? `is-filled${row.skill === "R" ? " is-ultimate" : ""}` : ""}>{level ?? ""}</b>)}
    </div>)}
  </div>;
}

/** Each rune tree's colour in the League client. */
const TREE_COLORS: Record<number, string> = { 8000: "#c8aa6e", 8100: "#d44242", 8200: "#9faafc", 8300: "#49aab9", 8400: "#a1d586" };

export function RunePage({ catalog, selection, onChange }: { catalog: RuneCatalog; selection: Selection; onChange?: (selection: Selection) => void }) {
  return <div className="fa-rune-page">{[selection.primaryPageId, selection.secondaryPageId].map((id, index) => {
    const tree = catalog.trees.find(tree => tree.id === id);
    if (!tree) return null;
    const selected = index === 0 ? [selection.keystone, ...selection.primaryRunes] : selection.secondaryRunes;
    return <div className="fa-rune-tree" key={id}>{onChange ? <Picker label={index ? "Secondary tree" : "Primary tree"} hideLabel className="fa-tree-picker" value={String(id)} options={catalog.trees.filter(tree => tree.id !== (index ? selection.primaryPageId : selection.secondaryPageId)).map(tree => ({ value: String(tree.id), label: tree.name, icon: <RuneIcon id={tree.id} mode="desktop" />, accent: TREE_COLORS[tree.id] }))} onChange={value => {
      const next = catalog.trees.find(tree => tree.id === Number(value))!;
      onChange(index ? { ...selection, secondaryPageId: next.id, secondaryRunes: next.rows.slice(0, 2).map(row => row.runes[0].id) } : { ...selection, primaryPageId: next.id, keystone: next.keystones[0].id, primaryRunes: next.rows.map(row => row.runes[0].id) });
    }} /> : <strong>{tree.name}</strong>}{index === 0 && <RuneRowDisplay row={tree.keystones} selected={selected} onPick={onChange ? id => onChange({ ...selection, keystone: id }) : undefined} />}{tree.rows.map((row, rowIndex) => <RuneRowDisplay key={rowIndex} row={row.runes} selected={selected} onPick={onChange ? id => onChange(index ? toggleSecondary(selection, tree, id) : togglePrimary(selection, tree, id)) : undefined} />)}{index === 1 && <div className="fa-shards">{catalog.shards.map((row, index) => <RuneRowDisplay key={index} row={row.runes} selected={row.runes.filter(rune => shardChosen(selection, index, rune.id)).map(rune => rune.id)} onPick={onChange ? id => onChange(selectShard(selection, index, id)) : undefined} />)}</div>}</div>;
  })}</div>;
}
function RuneRowDisplay({ row, selected, onPick }: { row: RuneRow["runes"]; selected: number[]; onPick?: (id: number) => void }) {
  return <div className="fa-rune-row">{row.map(rune => <button type="button" disabled={!onPick} key={rune.id} className={selected.includes(rune.id) ? "is-chosen" : ""} title={rune.name} aria-label={rune.name} aria-pressed={selected.includes(rune.id)} onClick={() => onPick?.(rune.id)}><NearViewport><RuneIcon id={rune.id} mode="desktop" /></NearViewport></button>)}</div>;
}

/** Keystone with its secondary tree, then the first two core items, like a pro build card. */
function BuildIcons({ keystone, secondary, items }: { keystone?: number; secondary?: number; items: { id: number; name: string; icon?: string }[] }) {
  return <NearViewport className="cs-build-icons">
    {keystone ? <span className="cs-row-runes"><RuneIcon id={keystone} mode="desktop" />{secondary ? <RuneIcon id={secondary} mode="desktop" className="cs-row-sub" /> : null}</span> : null}
    {items.slice(0, 2).map(item => <ItemIcon key={item.id} {...item} mode="desktop" />)}
  </NearViewport>;
}
