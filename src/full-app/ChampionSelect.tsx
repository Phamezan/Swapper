import { useEffect, useMemo, useState, type ComponentProps } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { DraftIcon } from "./ui/icons";
import { useAmbientChampion } from './AmbientArt';
import type { RunesPanel } from "../runes/RunesPanel";
import type { ChampionBuild, KeystoneBuildView, ChampionOverviewView, MatchupView, TierRow } from "../runes/types";
import { fmtPct, selectionFromPreset } from "../runes/types";
import { SpellIcon } from "../runes/SpellIcon";
import { RoleIcon } from "../runes/RoleIcon";
import { BuildPanel } from "./BuildPanel";
import { DraftBand } from "./ChampionSelectDraft";
import { PickSuggestions } from "./ChampionSelectPicks";
import { enemyLaners, pageMode, roleSlug, takenIds, type Draft } from "./champ-select-model";
import { mainRoles, ROLES } from "./championsModel";
import { Picker, RankPicker, ROLES as ROLE_OPTIONS } from "./Picker";
import { Notice } from "./ui/States";
import { fmtCount } from "./ui/format";
import "./champ-select.css";

/** Pick suggestions until lock-in, then the build workspace. */
export function ChampionSelect(props: ComponentProps<typeof RunesPanel>) {
  const view = props.view;
  useAmbientChampion('draft', view?.championId);
  const mode = pageMode(view);
  const [draft, setDraft] = useState<{ value: Draft; at: number } | null>(null);
  const [pickable, setPickable] = useState(new Set<number>());
  const [names, setNames] = useState<Map<number, string>>(new Map());
  const [items, setItems] = useState<KeystoneBuildView | null>(null);
  const [overview, setOverview] = useState<ChampionOverviewView | null>(null);
  const [roleLists, setRoleLists] = useState<Record<string, TierRow[]>>({});
  const [matchups, setMatchups] = useState<MatchupView[]>([]);
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
    setPickable(new Set());
    const stop = listen<Draft | null>("draft_changed", ({ payload }) => {
      received = true;
      if (live) setDraft(payload ? { value: payload, at: Date.now() } : null);
    });
    if (view?.phase === "ChampSelect") void stop.then(() => invoke<Draft>("full_app_draft")).then(value => {
      if (!live) return;
      setPickable(new Set(value.pickable ?? []));
      if (!received) setDraft({ value, at: Date.now() });
    }).catch(() => { if (live && !received) setDraft(null); });
    else setDraft(null);
    return () => { live = false; void stop.then(unlisten => unlisten()); };
  }, [view?.phase]);
  // Owned and free champions don't depend on whose turn it is; the list also hides the rest
  // from suggestions, so it is kept for the whole draft and refetched until the client fills it.
  useEffect(() => {
    if (!draft || pickable.size) return;
    let live = true;
    void invoke<Draft>("full_app_draft").then(value => { if (live) setPickable(new Set(value.pickable ?? [])); }).catch(() => {});
    return () => { live = false; };
  }, [draft?.value.canLock, !draft]);
  const [lastRole, setLastRole] = useState(() => { try { return localStorage.getItem("cs-role") || "mid"; } catch { return "mid"; } });
  const assigned = roleSlug(draft?.value.allies.find(member => member.local)?.position ?? "");
  // The role picked in this draft wins; otherwise League's assigned role (a role swap clears the pick so the list
  // follows it), otherwise the saved role. Never the backend's roleless guess, so hovering cannot swap the list.
  const [picked, setPicked] = useState<string | null>(null);
  useEffect(() => setPicked(null), [assigned]);
  const role = picked || assigned || lastRole;
  useEffect(() => {
    if (view?.championId && view.position && view.position !== role) props.onPositionChange(role);
  }, [view?.championId, view?.position, role]);
  const changeRole = (next: string) => {
    setPicked(next);
    setLastRole(next);
    try { localStorage.setItem("cs-role", next); } catch { /* preference only */ }
    props.onPositionChange(next);
  };
  const tier = view?.tier;
  // Who plays each role, to find your laner when ranked hides enemy positions. The backend caches the lists.
  useEffect(() => {
    if (!tier) return;
    let live = true;
    void Promise.all(ROLES.map(r => props.championApi.tierList(r, tier).then(view => [r, view.rows] as const).catch(() => [r, []] as const)))
      .then(entries => { if (live) setRoleLists(Object.fromEntries(entries)); });
    return () => { live = false; };
  }, [tier]);
  const mains = useMemo(() => mainRoles(roleLists), [roleLists]);
  const enemyIds = draft ? enemyLaners(draft.value, role, roleLists[role]) : view?.enemyChampionId ? [view.enemyChampionId] : [];
  // One known laner drives counters and cooldowns; several candidates each get a build tab to choose from.
  const enemyId = enemyIds.length === 1 ? enemyIds[0] : 0;
  const enemyKey = enemyIds.join(",");
  useEffect(() => {
    let live = true; setItems(null); setOverview(null); setError("");
    if (mode === "build" && view?.championId) {
      if (preset) void props.onLoadBuild(view.championId, view.position, view.tier, preset.keystone, selectionFromPreset(preset)).then(value => { if (live) setItems(value); }).catch(reason => { if (live) setError(String(reason)); });
      void props.championApi.overview(view.championId, view.position, view.tier).then(value => { if (live) setOverview(value); }).catch(reason => { if (live) setError(String(reason)); });
    }
    return () => { live = false; };
  }, [mode, view?.championId, view?.position, view?.tier, preset?.keystone]);
  useEffect(() => {
    let live = true; setMatchups([]);
    if (mode === "build" && view?.championId) void Promise.all(enemyIds.map(id => props.onLoadMatchup(view.championId, id, view.position, view.tier).catch(() => null)))
      .then(values => { if (live) setMatchups(values.filter((value): value is MatchupView => !!value)); });
    return () => { live = false; };
  }, [mode, view?.championId, view?.position, view?.tier, enemyKey]);
  const taken = useMemo(() => takenIds(draft?.value ?? null), [draft]);
  const build = useMemo<ChampionBuild | null>(() => preset && items ? { runes: preset, spells: preset.spells, items, skillOrder: items.skillOrder ?? overview?.build?.skillOrder ?? null } : overview?.build ?? null, [preset, items, overview?.build]);
  const enemyNameOf = (id: number) => names.get(id) || (id === view?.enemyChampionId ? view.enemyChampionName : "") || "";
  const enemyNames = enemyIds.map(enemyNameOf).filter(Boolean).join(" or ");
  const championName = view?.championName || names.get(view?.championId ?? 0) || "";

  // Five one-click role buttons (icons, like the client's role filter), then the rank picker.
  const pickers = <><div className="fa-seg role-tabs cs-role-buttons" role="group" aria-label="Role">{ROLE_OPTIONS.map(([value, label]) => <button key={value} type="button"
    aria-pressed={role === value} aria-label={label} title={label} onClick={() => changeRole(value)}><RoleIcon role={value} size={18} mode="desktop" /></button>)}</div>
    <RankPicker value={view?.tier ?? ""} onChange={props.onTierChange} /></>;
  const notices = <>{props.error && <Notice tone="error">{props.error}</Notice>}{error && <Notice tone="error">{error}</Notice>}</>;
  return <div className={`fa-draft-page dash-scope is-${mode}`}>
    {!view || mode === "idle" ? <>{notices}<section className="fa-card fa-draft-empty"><div className="fa-draft-emblem"><DraftIcon size={52} strokeWidth={1.25} /></div><h2>{props.loading ? "Connecting…" : "Waiting for draft"}</h2><div className="fa-draft-placeholder-team">{([["top", "Top"], ["jungle", "Jungle"], ["mid", "Mid"], ["adc", "Bot"], ["support", "Support"]] as const).map(([role, label]) => <span key={role}><RoleIcon role={role} size={26} mode="desktop" /><small>{label}</small></span>)}</div></section></> : <>
      <DraftBand draft={draft?.value ?? null} readAt={draft?.at ?? 0} names={names} enemyIds={enemyIds} mainRoles={mains} />
      <div className="cs-workspace">
      {notices}
      {mode === "planning" ? <div className="cs-planning">
        <PickSuggestions role={role} tier={view.tier} enemyIds={enemyIds} taken={taken} names={names} hovered={view.championId} tierList={props.championApi.tierList} controls={pickers} canLock={!!draft?.value.canLock} banTurn={draft?.value.banTurn} pickable={pickable} />
      </div> : <>
        <div className="fa-draft-controls cs-toolbar">
          <h2>{championName}{enemyNames && <small> vs {enemyNames}</small>}</h2>
          {enemyId > 0 && matchups[0]?.stats && <span className="cs-matchup-chip" title={`${fmtCount(matchups[0].stats.games)} games`}>{fmtPct(matchups[0].stats.winPct)} in this lane</span>}
          {pickers}
          {(["d", "f"] as const).map((slot, i) => <Picker key={slot} label={`Summoner ${slot.toUpperCase()}`} value={String(i ? view.spells.spell2Id : view.spells.spell1Id)} options={view.spells.available.map(spell => ({ value: String(spell.id), label: spell.name, icon: <SpellIcon id={spell.id} mode="desktop" /> }))} disabled={props.busy} onChange={value => props.onPickSpell(slot, Number(value))} />)}
        </div>
        <BuildPanel championId={view.championId} name={championName} lane={view.position} tier={view.tier} build={build} loading={!overview && !error} liveView={view} matchups={matchups} proPageSize={3}
          autoApply={{ checked: view.autoApply, disabled: props.busy, onChange: props.onToggleAutoApply }} />
      </>}
      {props.shopNotice && !props.shopNotice.ok && <Notice>{props.shopNotice.message}</Notice>}
      </div>
    </>}
  </div>;
}
