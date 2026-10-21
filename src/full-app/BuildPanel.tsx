import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ArrowRight, Check, ChevronRight } from "lucide-react";
import { ItemIcon } from "../runes/ItemIcon";
import { RuneIcon } from "../runes/RuneIcon";
import { SpellIcon } from "../runes/SpellIcon";
import { CachedBadge } from "../runes/CachedBadge";
import { Switch } from "../components/ui/switch";
import { fmtPct, selectionFromPreset, selectionFromProBuild, shopNoticeFrom, togglePrimary, toggleSecondary, selectShard } from "../runes/types";
import type { ChampionBuild, ItemsImported, KeystoneBuildView, MatchupView, PresetBuildView, ProBuild, ProBuildsView, RuneRow, RuneTree, RunesView, Selection } from "../runes/types";
import { overviewSelection } from "./buildLogic";
import { applyStatus, itemSetKey, maxOrder, skillGrid, type ApplyStatus } from "./champ-select-model";
import { NearViewport } from "./NearViewport";
import { Picker } from "./Picker";
import "./champ-select.css";

export type RuneCatalog = { trees: RuneTree[]; shards: RuneRow[] };
type Ability = { key: string; name: string; icon: string | null };
type Variant = { label: string; selection: Selection | null; items: KeystoneBuildView; spells: number[]; order: string | null; winPct: number | null; games: number };
type Section = keyof ApplyStatus;
/** The auto-apply setting, shown inline on the champion select page. */
export type AutoApply = { checked: boolean; disabled: boolean; onChange: (enabled: boolean) => void };

export function BuildPanel({ championId, name, lane, tier, build, liveView, matchup, autoApply }: {
  championId: number; name: string; lane: string; tier: string; build: ChampionBuild | null; liveView?: RunesView | null;
  /** The lane matchup; adds a "vs <enemy>" tab when it has its own build. */
  matchup?: MatchupView | null; autoApply?: AutoApply;
}) {
  const [catalog, setCatalog] = useState<RuneCatalog | null>(null);
  const [details, setDetails] = useState<PresetBuildView | null>(null);
  const [abilities, setAbilities] = useState<Ability[]>([]);
  const [variantIndex, setVariantIndex] = useState(0);
  const [client, setClient] = useState<RunesView | null>(liveView ?? null);
  const [busy, setBusy] = useState<Section | "all" | null>(null);
  const [notice, setNotice] = useState("");
  const [imported, setImported] = useState<string | null>(null);
  const [proOpen, setProOpen] = useState(false);
  const [pros, setPros] = useState<ProBuildsView | null>(null);
  const [pro, setPro] = useState<ProBuild | null>(null);
  const [proError, setProError] = useState("");
  const [editing, setEditing] = useState(false);
  const [custom, setCustom] = useState<Selection | null>(null);
  useEffect(() => {
    let live = true;
    setImported(null);
    void invoke<RuneCatalog>("full_app_rune_catalog").then(value => { if (live) setCatalog(value); }).catch(() => {});
    void invoke<Ability[]>("full_app_abilities", { championId }).then(value => { if (live) setAbilities(value); }).catch(() => {});
    const update = () => invoke<RunesView>("get_runes").then(value => { if (live) setClient(value); }).catch(() => {});
    if (!liveView) void update();
    const events = listen("champ_select", () => { void update(); });
    const items = listen<ItemsImported>("items_imported", ({ payload }) => { if (live) setNotice(shopNoticeFrom(payload).message); });
    return () => { live = false; void events.then(stop => stop()); void items.then(stop => stop()); };
  }, [championId]);
  useEffect(() => { if (liveView) setClient(liveView); }, [liveView]);
  const selection = overviewSelection(build, catalog?.trees ?? []);
  useEffect(() => {
    let live = true;
    setDetails(null); setVariantIndex(0); setPro(null); setPros(null); setProOpen(false); setNotice(""); setCustom(null); setEditing(false);
    if (build?.runes) void invoke<PresetBuildView | null>("get_keystone_build", { championId, position: lane, tier, keystone: build.runes.keystone, mostPlayed: selection })
      .then(value => { if (live) setDetails(value); }).catch(() => {});
    return () => { live = false; };
  }, [championId, lane, tier, build, catalog]);
  useEffect(() => {
    if (!proOpen) return;
    let live = true;
    setProError("");
    void invoke<ProBuildsView>("get_pro_builds", { championId, position: lane, page: 1, isOtp: false })
      .then(value => { if (live) setPros(value); }).catch(error => { if (live) setProError(String(error)); });
    return () => { live = false; };
  }, [proOpen, championId, lane]);

  if (!build) return <section className="fa-card fa-build-empty"><h3>Build unavailable</h3><p>Try another role or rank. Recommendations appear when the provider has a build for this pick.</p></section>;
  const variants: Variant[] = [{ label: "Most played", selection, items: details ?? build.items, spells: build.spells, order: build.skillOrder, winPct: (details ?? build.items).coreWinPct, games: (details ?? build.items).coreGames }];
  if (details?.highestWin && !details.highestWinMatchesPick) {
    const high = details.highestWin;
    variants.push({ label: "Highest win rate", selection: selectionFromPreset(high.preset), items: high.build, spells: high.preset.spells, order: high.build.skillOrder ?? null, winPct: high.build.coreWinPct, games: high.build.coreGames });
  }
  if (matchup?.preset && matchup.build && !matchup.fallback) {
    variants.push({ label: `vs ${matchup.enemyName}`, selection: selectionFromPreset(matchup.preset), items: matchup.build, spells: matchup.preset.spells, order: matchup.build.skillOrder ?? null, winPct: matchup.stats?.winPct ?? matchup.build.coreWinPct, games: matchup.stats?.games ?? matchup.build.coreGames });
  }
  const chosen: Variant = pro ? { label: pro.proName, selection: selectionFromProBuild(pro), items: { ...build.items, items: pro.completedItems, starters: [], options: [], coreLen: pro.completedItems.length, coreGames: 0, coreWinPct: null, skillPriority: null }, spells: pro.spells, order: pro.skillOrder.join(""), winPct: null, games: 1 } : variants[variantIndex] ?? variants[0];
  const chosenSelection = custom ?? chosen.selection;
  const complete = !!chosenSelection && chosenSelection.primaryRunes.length === 3 && chosenSelection.secondaryRunes.length === 2 && chosenSelection.shards.length === 3;
  const inSelect = client?.phase === "ChampSelect" && client.championId === championId;
  const itemIds = chosen.items.items.map(item => item.id);
  const itemsKey = itemSetKey(championId, itemIds);
  const status = applyStatus(client, championId, complete ? chosenSelection : null, chosen.spells, itemsKey, imported);
  const ready: ApplyStatus = { runes: inSelect && complete, spells: inSelect && chosen.spells.length === 2, items: itemIds.length > 0 };
  const boots = chosen.items.items.filter(item => /boots|greaves|treads|shoes|steelcaps|soulmarch/i.test(item.name));
  const order = maxOrder(chosen.order, chosen.items.skillPriority);
  const showTab = (index: number | null) => { setPro(null); setProOpen(index === null); if (index !== null) setVariantIndex(index); setCustom(null); setEditing(false); };

  const apply = async (section: Section | "all") => {
    setBusy(section); setNotice("");
    try {
      const runes = section === "runes" || (section === "all" && ready.runes && !status.runes);
      const spells = section === "spells" || (section === "all" && ready.spells && !status.spells);
      const items = section === "items" || (section === "all" && ready.items && !status.items);
      if (runes || spells) {
        const current = await invoke<RunesView>("get_runes");
        setClient(current);
        if (current.phase !== "ChampSelect" || current.championId !== championId) throw new Error(`Select ${name} in League Client before applying this build.`);
      }
      if (runes) await invoke("apply_rune_page", { selection: chosenSelection, presetIndex: null, spells: spells ? chosen.spells : null, position: lane, enemyChampionId: null, highestWin: false });
      else if (spells) for (const [index, slot] of (["d", "f"] as const).entries()) await invoke("apply_spell", { slot, spellId: chosen.spells[index] });
      if (items) { await invoke("import_item_build", { championId, championName: name, items: itemIds }); setImported(itemsKey); }
      if (runes || spells) setClient(await invoke<RunesView>("get_runes"));
    } catch (error) { setNotice(String(error)); }
    finally { setBusy(null); }
  };
  const allDone = (!ready.runes || status.runes) && (!ready.spells || status.spells) && (!ready.items || status.items);
  const sections: [Section, string][] = [["runes", "Runes"], ["spells", "Spells"], ["items", "Item set"]];

  return <div className="fa-build-section">
    <div className="fa-build-tabs" role="group" aria-label="Build variant">{variants.map((variant, index) => <button key={variant.label} aria-pressed={!proOpen && index === variantIndex} onClick={() => showTab(index)}>
      <span className="fa-item-line">{variant.items.items.slice(0, 2).map(item => <ItemIcon key={item.id} {...item} mode="desktop" />)}</span><span><strong>{variant.label}</strong><small>{variant.games.toLocaleString()} games</small></span><b>{fmtPct(variant.winPct)}</b>
    </button>)}
      <button aria-pressed={proOpen} onClick={() => showTab(null)}><span><strong>Pro builds</strong><small>{pro ? `${pro.proName} · ${pro.patch}` : "Recent pro games"}</small></span></button>
    </div>
    <div className="cs-apply-bar" aria-label="Apply to League Client">
      {sections.map(([key, label]) => <span key={key} className={`cs-apply-chip${status[key] ? " is-done" : ""}`}>
        {status[key] ? <><Check size={14} aria-hidden /><strong>{label}</strong><small>{key === "runes" ? "Applied" : key === "items" ? "Added" : "Set"}</small></>
          : <><strong>{label}</strong><button className="fa-text-button" disabled={!ready[key] || !!busy} title={ready[key] ? `Apply ${label.toLowerCase()} to League Client` : `Select ${name} in champion select to apply`} onClick={() => void apply(key)}>{busy === key ? "Applying…" : "Apply"}</button></>}
      </span>)}
      {autoApply && <label className="cs-auto-apply">Auto-apply<Switch aria-label="Auto-apply runes" checked={autoApply.checked} disabled={autoApply.disabled} onCheckedChange={autoApply.onChange} /></label>}
      <button className="fa-primary" disabled={!!busy || allDone || !(ready.runes || ready.spells || ready.items)} onClick={() => void apply("all")}><Check size={16} />{busy === "all" ? "Applying…" : allDone ? "All applied" : "Apply all"}</button>
    </div>
    {notice && <p className="fa-notice" role="status">{notice}</p>}
    {proOpen && <div className="fa-pro-grid">{proError ? <p role="alert">{proError}</p> : !pros ? <p role="status">Loading pro builds…</p> : !pros.matches.length ? <p>No recent pro builds for this role.</p> : pros.matches.slice(0, 8).map(match => <button className="fa-pro-card" key={match.matchId} aria-pressed={pro?.matchId === match.matchId} onClick={() => { setPro(match); setCustom(null); }}><strong>{match.proName} <small>{match.team}</small></strong><span>{match.kills} / {match.deaths} / {match.assists} · {match.win ? "Win" : "Loss"} · {match.playedAgo}</span><span className="fa-item-line">{match.finalItems.slice(0, 6).map((item, index) => <ItemIcon key={index} {...item} mode="desktop" />)}</span></button>)}</div>}
    {proOpen && !pro ? null : <section className="fa-card fa-build-grid cs-build-grid">
      <div className="fa-rune-area"><div className="fa-section-heading"><h4>{custom ? "Custom rune page" : "Rune page"}</h4><button className="fa-text-button" disabled={!chosenSelection || !!busy} aria-pressed={editing} onClick={() => setEditing(!editing)}>{editing ? "Done" : "Edit runes"}</button></div>{catalog && chosenSelection ? <RunePage catalog={catalog} selection={chosenSelection} onChange={editing ? setCustom : undefined} /> : <div className="fa-rune-fallback"><p>Rune catalog unavailable</p>{build.runes && [build.runes.keystone, ...build.runes.primaryRunes, ...build.runes.secondaryRunes, ...build.runes.shards].map(id => <RuneIcon key={id} id={id} mode="desktop" />)}</div>}</div>
      <div className="fa-item-area"><h4>Item build</h4>
        <h5>Starting items</h5><div className="fa-item-line">{chosen.items.starters.map(item => <span className="fa-item-stack" key={item.id}><ItemIcon {...item} mode="desktop" />{item.count > 1 && <b>×{item.count}</b>}</span>)}{!chosen.items.starters.length && <small>Not reported</small>}</div>
        {boots.length > 0 && <><h5>Boots</h5><div className="fa-item-line">{boots.map(item => <ItemIcon key={item.id} {...item} mode="desktop" />)}</div></>}
        <h5>Core path <small>{fmtPct(chosen.items.coreWinPct)} · {chosen.items.coreGames.toLocaleString()} games</small></h5><div className="fa-item-path">{chosen.items.items.slice(0, chosen.items.coreLen || 3).map((item, index) => <span key={`${item.id}-${index}`}>{index > 0 && <ArrowRight size={14} />}<ItemIcon {...item} mode="desktop" /></span>)}</div>
        <h5>Later items <small>Popular choices for each slot</small></h5><div className="fa-item-line">{chosen.items.items.slice(chosen.items.coreLen || 3).map(item => <ItemIcon key={item.id} {...item} mode="desktop" />)}</div>
        {chosen.items.options.length > 0 && <><h5>Situational</h5><div className="fa-item-line">{chosen.items.options.slice(0, 7).map(item => <span key={`${item.id}-${item.slot}`} title={`${item.name} · slot ${item.slot} · ${fmtPct(item.winPct)} · ${item.games.toLocaleString()} games`}><ItemIcon {...item} mode="desktop" /></span>)}</div></>}
        {chosen.items.stale && <CachedBadge stale updatedAt={chosen.items.updatedAt} />}
      </div>
      <div className="fa-spell-area cs-skill-area"><div><h4>Summoner spells</h4><div className="fa-spells">{chosen.spells.map((id, index) => <span key={index}><SpellIcon id={id} mode="desktop" /><kbd>{index ? "F" : "D"}</kbd></span>)}</div></div>
        <div><h4>Skill order</h4>
        {order.length ? <div className="fa-skill-priority" aria-label={`Max order ${order.join(", ")}`}>{order.map((key, index) => <span key={key}>{index > 0 && <ChevronRight size={14} />}<AbilityBadge ability={abilities.find(ability => ability.key === key)} skill={key} /></span>)}</div> : <p>Max order unavailable</p>}
        {chosen.order ? <SkillOrderGrid order={chosen.order} abilities={abilities} /> : <p>Level order unavailable</p>}</div>
      </div>
    </section>}
  </div>;
}

function AbilityBadge({ ability, skill }: { ability?: Ability; skill: string }) {
  return <span className="fa-ability" title={ability?.name ?? skill}>{ability?.icon && <img src={ability.icon} alt={ability.name} loading="lazy" />}<kbd>{skill}</kbd></span>;
}

function SkillOrderGrid({ order, abilities }: { order: string; abilities: Ability[] }) {
  return <div className="cs-skill-grid" role="table" aria-label={`Skill order by level: ${order.split("").join(" ")}`}>
    {skillGrid(order).map(row => <div className="cs-skill-row" role="row" key={row.skill}>
      <span role="rowheader" className={row.skill === "R" ? "is-ultimate" : ""} title={abilities.find(ability => ability.key === row.skill)?.name}>{row.skill}</span>
      {row.cells.map((level, index) => <b role="cell" key={index} className={level ? row.skill === "R" ? "is-filled is-ultimate" : "is-filled" : ""}>{level ?? ""}</b>)}
    </div>)}
  </div>;
}

export function RunePage({ catalog, selection, onChange }: { catalog: RuneCatalog; selection: Selection; onChange?: (selection: Selection) => void }) {
  return <div className="fa-rune-page">{[selection.primaryPageId, selection.secondaryPageId].map((id, index) => {
    const tree = catalog.trees.find(tree => tree.id === id);
    if (!tree) return null;
    const selected = index === 0 ? [selection.keystone, ...selection.primaryRunes] : selection.secondaryRunes;
    return <div className="fa-rune-tree" key={id}>{onChange ? <Picker label={index ? "Secondary tree" : "Primary tree"} value={String(id)} options={catalog.trees.filter(tree => tree.id !== (index ? selection.primaryPageId : selection.secondaryPageId)).map(tree => ({ value: String(tree.id), label: tree.name }))} onChange={value => {
      const next = catalog.trees.find(tree => tree.id === Number(value))!;
      onChange(index ? { ...selection, secondaryPageId: next.id, secondaryRunes: next.rows.slice(0, 2).map(row => row.runes[0].id) } : { ...selection, primaryPageId: next.id, keystone: next.keystones[0].id, primaryRunes: next.rows.map(row => row.runes[0].id) });
    }} /> : <strong>{tree.name}</strong>}{index === 0 && <RuneRowDisplay row={tree.keystones} selected={selected} onPick={onChange ? id => onChange({ ...selection, keystone: id }) : undefined} />}{tree.rows.map((row, rowIndex) => <RuneRowDisplay key={rowIndex} row={row.runes} selected={selected} onPick={onChange ? id => onChange(index ? toggleSecondary(selection, tree, id) : togglePrimary(selection, tree, id)) : undefined} />)}{index === 1 && <div className="fa-shards">{catalog.shards.map((row, index) => <RuneRowDisplay key={index} row={row.runes} selected={selection.shards} onPick={onChange ? id => onChange(selectShard(selection, index, id)) : undefined} />)}</div>}</div>;
  })}</div>;
}
function RuneRowDisplay({ row, selected, onPick }: { row: RuneRow["runes"]; selected: number[]; onPick?: (id: number) => void }) {
  return <div className="fa-rune-row">{row.map(rune => <button type="button" disabled={!onPick} key={rune.id} className={selected.includes(rune.id) ? "is-chosen" : ""} title={rune.name} aria-label={rune.name} aria-pressed={selected.includes(rune.id)} onClick={() => onPick?.(rune.id)}><NearViewport><RuneIcon id={rune.id} mode="desktop" /></NearViewport></button>)}</div>;
}
