import { useEffect, useMemo, useState, type ComponentProps } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { DraftIcon } from "./ui/icons";
import type { RunesPanel } from "../runes/RunesPanel";
import type { ChampionBuild, KeystoneBuildView, ChampionOverviewView, MatchupView } from "../runes/types";
import { fmtPct, selectionFromPreset } from "../runes/types";
import { ItemIcon } from "../runes/ItemIcon";
import { RuneIcon } from "../runes/RuneIcon";
import { SpellIcon } from "../runes/SpellIcon";
import { RoleIcon } from "../runes/RoleIcon";
import { BuildPanel } from "./BuildPanel";
import { CooldownSheet } from "./CooldownSheet";
import { DraftBand } from "./ChampionSelectDraft";
import { ChampionArt } from "./MatchupStrip";
import { NearViewport } from "./NearViewport";
import { LockIn, PickSuggestions } from "./ChampionSelectPicks";
import { enemyLaner, pageMode, roleSlug, takenIds, type Draft } from "./champ-select-model";
import { Picker, RankPicker, RolePicker } from "./Picker";
import { PageHeader } from "./ui/PageHeader";
import { Notice, Skeleton } from "./ui/States";
import { fmtCount } from "./ui/format";
import "./champ-select.css";

/** Planning mode before your pick is locked in (suggestions large, build
 *  small), build mode after (build large). The mode follows the live draft. */
export function ChampionSelect(props: ComponentProps<typeof RunesPanel>) {
  const view = props.view;
  const mode = pageMode(view);
  const [draft, setDraft] = useState<{ value: Draft; at: number } | null>(null);
  const [names, setNames] = useState<Map<number, string>>(new Map());
  const [items, setItems] = useState<KeystoneBuildView | null>(null);
  const [overview, setOverview] = useState<ChampionOverviewView | null>(null);
  const [matchup, setMatchup] = useState<MatchupView | null>(null);
  const [error, setError] = useState("");
  const preset = view?.presets[0];
  useEffect(() => {
    let live = true;
    void props.championApi.list().then(list => { if (live) setNames(new Map(list.map(champion => [champion.id, champion.name]))); }).catch(() => {});
    return () => { live = false; };
  }, []);
  useEffect(() => {
    let live = true;
    let received = false;
    const stop = listen<Draft | null>("draft_changed", ({ payload }) => {
      received = true;
      if (live) setDraft(payload ? { value: payload, at: Date.now() } : null);
    });
    if (view?.phase === "ChampSelect") void stop.then(() => invoke<Draft>("full_app_draft")).then(value => { if (live && !received) setDraft({ value, at: Date.now() }); }).catch(() => { if (live && !received) setDraft(null); });
    else setDraft(null);
    return () => { live = false; void stop.then(unlisten => unlisten()); };
  }, [view?.phase]);
  const [lastRole, setLastRole] = useState(() => { try { return localStorage.getItem("cs-role") || "mid"; } catch { return "mid"; } });
  const assigned = roleSlug(draft?.value.allies.find(member => member.local)?.position ?? "");
  const role = view?.position || assigned || lastRole;
  const changeRole = (next: string) => {
    setLastRole(next);
    try { localStorage.setItem("cs-role", next); } catch { /* preference only */ }
    props.onPositionChange(next);
  };
  const enemyId = draft ? enemyLaner(draft.value, role) : view?.enemyChampionId ?? 0;
  useEffect(() => {
    let live = true; setItems(null); setOverview(null); setError("");
    if (view?.championId) {
      if (preset) void props.onLoadBuild(view.championId, view.position, view.tier, preset.keystone, selectionFromPreset(preset)).then(value => { if (live) setItems(value); }).catch(reason => { if (live) setError(String(reason)); });
      void props.championApi.overview(view.championId, view.position, view.tier).then(value => { if (live) setOverview(value); }).catch(reason => { if (live) setError(String(reason)); });
    }
    return () => { live = false; };
  }, [view?.championId, view?.position, view?.tier, preset?.keystone]);
  useEffect(() => {
    let live = true; setMatchup(null);
    if (view?.championId && enemyId) void props.onLoadMatchup(view.championId, enemyId, view.position, view.tier)
      .then(value => { if (live) setMatchup(value); }).catch(() => {});
    return () => { live = false; };
  }, [view?.championId, view?.position, view?.tier, enemyId]);
  const taken = useMemo(() => takenIds(draft?.value ?? null), [draft]);
  const build = useMemo<ChampionBuild | null>(() => preset && items ? { runes: preset, spells: preset.spells, items, skillOrder: items.skillOrder ?? overview?.build?.skillOrder ?? null } : overview?.build ?? null, [preset, items, overview?.build]);
  const enemyName = names.get(enemyId) || (enemyId === view?.enemyChampionId ? view.enemyChampionName : "") || "";
  const championName = view?.championName || names.get(view?.championId ?? 0) || "";

  return <div className="fa-draft-page dash-scope">
    <PageHeader title="Draft" actions={mode !== "idle" && <span className="fa-status-pill is-live">{mode === "build" ? "Locked in" : "Live draft"}</span>} />
    {props.error && <Notice tone="error">{props.error}</Notice>}{error && <Notice tone="error">{error}</Notice>}
    {!view || mode === "idle" ? <section className="fa-card fa-draft-empty"><div className="fa-draft-emblem"><DraftIcon size={52} strokeWidth={1.25} /></div><h2>{props.loading ? "Connecting…" : "Waiting for draft"}</h2><div className="fa-draft-placeholder-team">{([["top", "Top"], ["jungle", "Jungle"], ["mid", "Mid"], ["adc", "Bot"], ["support", "Support"]] as const).map(([role, label]) => <span key={role}><RoleIcon role={role} size={26} mode="desktop" /><small>{label}</small></span>)}</div></section> : <>
      <DraftBand draft={draft?.value ?? null} readAt={draft?.at ?? 0} names={names} enemyId={enemyId} small={mode === "build"} />
      <div className="fa-draft-controls cs-toolbar">
        <h2>{championName || "Choose your champion"}{enemyName && <small> vs {enemyName}</small>}</h2>
        {matchup?.stats && <span className="cs-matchup-chip" title={`${fmtCount(matchup.stats.games)} games`}>{fmtPct(matchup.stats.winPct)} in this lane</span>}
        <RolePicker value={role} onChange={changeRole} /><RankPicker value={view.tier} onChange={props.onTierChange} />
        {mode === "build" && (["d", "f"] as const).map((slot, i) => <Picker key={slot} label={`Summoner ${slot.toUpperCase()}`} value={String(i ? view.spells.spell2Id : view.spells.spell1Id)} options={view.spells.available.map(spell => ({ value: String(spell.id), label: spell.name, icon: <SpellIcon id={spell.id} mode="desktop" /> }))} disabled={props.busy} onChange={value => props.onPickSpell(slot, Number(value))} />)}
      </div>
      {mode === "planning" ? <div className="cs-planning">
        <PickSuggestions role={role} tier={view.tier} enemyId={enemyId} enemyName={enemyName} enemyCount={draft?.value.enemies.filter(member => member.championId > 0).length ?? 0} taken={taken} names={names} hovered={view.championId} tierList={props.championApi.tierList} />
        <BuildPreview championId={view.championId} name={championName} build={build} />
      </div> : <>
        <BuildPanel championId={view.championId} name={championName} lane={view.position} tier={view.tier} build={build} liveView={view} matchup={matchup}
          autoApply={{ checked: view.autoApply, disabled: props.busy, onChange: props.onToggleAutoApply }} />
        {enemyId > 0 && <CooldownSheet championId={enemyId} />}
      </>}
      {props.shopNotice && <Notice>{props.shopNotice.message}</Notice>}
    </>}
  </div>;
}

/** Planning mode's small build: what the hovered champion would run. */
function BuildPreview({ championId, name, build: value }: { championId: number; name: string; build: ChampionBuild | null }) {
  return <aside className="fa-card cs-preview" aria-label="Build preview">
    <div className={`cs-preview-hero${!championId ? " is-empty" : ""}`}>
      {championId > 0 && <NearViewport className="cs-preview-art"><ChampionArt id={championId} name={name} /></NearViewport>}
      <div className="cs-preview-identity"><small>Your pick</small><h3>{name || "Choose a champion"}</h3><span>{championId ? "Recommended build" : "Preview before locking in"}</span></div>
    </div>
    <LockIn championId={championId} name={name} />
    <div className="cs-preview-body">
    {!championId ? <p>Hover a champion to preview its build.</p>
      : !value ? <Skeleton label="Loading build" rows={4} /> : <>
        {value.runes && <div className="cs-preview-row"><small>Runes</small><span>{[value.runes.keystone, ...value.runes.primaryRunes, ...value.runes.secondaryRunes].map(id => <RuneIcon key={id} id={id} mode="desktop" />)}</span></div>}
        <div className="cs-preview-row"><small>Spells</small><span>{value.spells.map((id, index) => <SpellIcon key={index} id={id} mode="desktop" />)}</span></div>
        <div className="cs-preview-row"><small>Core <b>{fmtPct(value.items.coreWinPct)}</b></small><span>{value.items.items.slice(0, value.items.coreLen || 3).map((item, index) => <ItemIcon key={`${item.id}-${index}`} {...item} mode="desktop" />)}</span></div>
      </>}
    </div>
  </aside>;
}
