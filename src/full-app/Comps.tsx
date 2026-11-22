import { createContext, useContext, useEffect, useMemo, useRef, useState } from "react";
import type { VodRequest } from "./MatchupVodEntry";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { ChampionIcon } from "../runes/ChampionIcon";
import { ChampionPicker } from "./Picker";
import type { ChampionOption, TierListView } from "../runes/types";
import { mainRoles, ROLES as TIER_ROLES } from "./championsModel";
import { ChampionArt } from "./MatchupStrip";
import { Notice, Skeleton } from "./ui/States";
import { PageHeader } from "./ui/PageHeader";
import { ChevronLeft, ChevronRight } from "lucide-react";
import { ANSWERS, CURRENT_PATCH, PRO_META, STYLES, VARIATIONS, championId } from "./compsData";
import { addedChampionRole, applyOverrides, assignRoles, compPlayable, coverageFor, isTailored, lobbyActive, partitionPlayable, peakLabel, playerLabel, subHintFor, teamMode, ROLES, ROLE_LABEL, type CompOption, type CompSlot, type Coverage, type PoolEntry, type Role, type RosterMember, type TeamPlayer, type Variation } from "./compsModel";
import { CompsTeamBar } from "./CompsTeamBar";
import type { Snapshot } from "./types";
import { CompsAnswers } from "./CompsAnswers";
import "./comps.css";

type Member = { puuid: string; riotId: string; games?: number };
type Context = { me: Member | null; lobby: Member[]; roster: RosterMember[]; suggested: { puuid: string; riotId: string; games: number }[]; overrides: { added: { championId: number; role: string }[]; hidden: number[] } };
type PoolResp = { puuid: string; entries: PoolEntry[]; error: string | null; profileIconId?: number | null };
const CDRAGON = "https://raw.communitydragon.org/latest/plugins/rcp-fe-lol-shared-components/global/default";
/** Mastery crest art from CommunityDragon; levels above 10 reuse the level-10 crest. */
const masteryCrest = (level: number) => `${CDRAGON}/mastery-${Math.min(10, Math.max(1, level))}.png`;
const profileIcon = (id: number) => `https://cdn.communitydragon.org/latest/profile-icon/${id}`;
/** A player's name without the tag; "Player" when the client gave no name (never the raw PUUID). */
const shortName = (p: { riotId: string; puuid: string }) => (p.riotId && p.riotId !== p.puuid ? p.riotId.split("#")[0] : "Player");

const OpenVods = createContext<((request: VodRequest) => void) | null>(null);

/** Champion tooltip: its name, plus the icon and name of the teammate who plays it. */
function ChampTip({ name, player }: { name: string; player?: TeamPlayer | null }) {
  return <span className="cp-tip"><b>{name}</b>{player && <span className="cp-tip-who">{player.profileIconId != null && <img src={profileIcon(player.profileIconId)} alt="" />}{player.isSelf ? "You" : shortName(player)}</span>}</span>;
}

/** Comps per column page on the overview. */
const PAGE_SIZE = 5;


export function Comps({ snapshot, champions, signedIn, onOpenVods, homeSignal }: { snapshot: Snapshot | null; champions: ChampionOption[]; signedIn: boolean; onOpenVods?: (request: VodRequest) => void; homeSignal?: number }) {
  const [tab, setTab] = useState<"comps" | "answers" | "meta">("comps");
  const [hideRest, setHideRest] = useState(false);
  const [peekLanes, setPeekLanes] = useState<Set<string>>(new Set());
  const [selected, setSelected] = useState<Variation | null>(null);
  const [poolsOpen, setPoolsOpen] = useState(false);
  const [ctx, setCtx] = useState<Context | null>(null);
  const [rosterMembers, setRosterMembers] = useState<RosterMember[]>([]);
  const [activePuuids, setActivePuuids] = useState<string[]>([]);
  const [roleOverrides, setRoleOverrides] = useState<Record<string, Role>>({});
  const [pools, setPools] = useState<Map<string, PoolResp>>(new Map());
  const [loadingPools, setLoadingPools] = useState(false);
  const activeInit = useRef(false);
  const lastLobbyKey = useRef("");
  const [pageOf, setPageOf] = useState<Record<string, number>>({});
  const [overrides, setOverrides] = useState<Context["overrides"]>({ added: [], hidden: [] });
  const [teamLoading, setTeamLoading] = useState(isTauri());
  const [saveError, setSaveError] = useState("");
  const overrideSaveChain = useRef<Promise<unknown>>(Promise.resolve());
  const accountKey = snapshot?.summoner.riotId ?? "";

  useEffect(() => {
    if (!isTauri()) { setTeamLoading(false); return; }
    let live = true;
    const load = () => { setTeamLoading(true); return invoke<Context>("comps_team").then(c => { if (live) { setCtx(c); setOverrides(c.overrides); } }).catch(() => { if (live) setCtx({ me: null, lobby: [], roster: [], suggested: [], overrides: { added: [], hidden: [] } }); }).finally(() => { if (live) setTeamLoading(false); }); };
    void load();
    const onFocus = () => { if (document.visibilityState === "visible") void load(); };
    window.addEventListener("focus", onFocus); document.addEventListener("visibilitychange", onFocus);
    return () => { live = false; window.removeEventListener("focus", onFocus); document.removeEventListener("visibilitychange", onFocus); };
  }, [signedIn, accountKey]);
  // Roster comes from the backend store; You are active by default.
  useEffect(() => { if (ctx) setRosterMembers(ctx.roster ?? []); }, [ctx]);
  useEffect(() => { if (ctx?.me && !activeInit.current) { activeInit.current = true; setActivePuuids([ctx.me.puuid]); } }, [ctx]);
  // Lobby merge: roster members in a new lobby become active; manual toggles win until the lobby changes.
  useEffect(() => {
    if (!ctx) return;
    const key = ctx.lobby.map(m => m.puuid).sort().join(",");
    if (!key || key === lastLobbyKey.current) return;
    lastLobbyKey.current = key;
    setActivePuuids(prev => lobbyActive(rosterMembers.map(r => r.puuid), ctx.lobby.map(m => m.puuid), prev.length ? prev : (ctx.me ? [ctx.me.puuid] : [])));
  }, [ctx, rosterMembers]);

  useEffect(() => {
    const puuids = [...(ctx?.me ? [ctx.me.puuid] : []), ...rosterMembers.map(m => m.puuid)];
    if (!puuids.length || !isTauri()) return;
    let live = true; setLoadingPools(true);
    void invoke<PoolResp[]>("comps_pools", { puuids }).then(rows => { if (live) setPools(new Map(rows.map(r => [r.puuid, r]))); })
      .catch(() => { if (live) setPools(new Map(puuids.map(id => [id, { puuid: id, entries: [], error: "Pool unavailable" }]))); })
      .finally(() => { if (live) setLoadingPools(false); });
    return () => { live = false; };
  }, [rosterMembers, ctx?.me?.puuid]);

  const [champRoles, setChampRoles] = useState<Map<number, Role>>(new Map());
  useEffect(() => {
    if (!isTauri()) return;
    let live = true;
    void Promise.all(TIER_ROLES.map(r => invoke<TierListView>("get_tier_list", { position: r, tier: "emerald_plus" }).then(v => [r, v.rows] as const).catch(() => [r, []] as const)))
      .then(entries => { if (live) setChampRoles(new Map([...mainRoles(Object.fromEntries(entries))].map(([id, r]) => [id, (r === "adc" ? "bot" : r) as Role]))); });
    return () => { live = false; };
  }, []);
  const champName = useMemo(() => { const m = new Map(champions.map(c => [c.id, c.name])); if (snapshot) for (const [id, n] of Object.entries(snapshot.championNames)) m.set(Number(id), n); return m; }, [champions, snapshot]);
  // Full roster plus You; `included` marks the active five for coverage and the Answers tab.
  const teamBase: TeamPlayer[] = useMemo(() => {
    const members: (RosterMember & { isSelf: boolean })[] = [...(ctx?.me ? [{ puuid: ctx.me.puuid, riotId: ctx.me.riotId, nickname: null, roles: [] as Role[], isSelf: true }] : []), ...rosterMembers.map(m => ({ ...m, isSelf: false }))];
    return members.map(m => {
      const resp = pools.get(m.puuid);
      let pool: PoolEntry[] | null = resp ? (resp.error && !resp.entries.length ? null : resp.entries.map(e => e.role ? e : { ...e, role: champRoles.get(e.championId) ?? e.role })) : null;
      if (m.isSelf && pool) pool = applyOverrides(pool, overrides.added.map(a => ({ championId: a.championId, role: (a.role || "mid") as Role })), overrides.hidden);
      const roleCounts = new Map<string, number>();
      for (const e of pool ?? []) if (e.role) roleCounts.set(e.role, (roleCounts.get(e.role) ?? 0) + (e.games ?? 1));
      const poolRoles = [...roleCounts.entries()].sort((a, b) => b[1] - a[1]).map(([r]) => r as Role);
      const roles = m.isSelf ? poolRoles : (m.roles.length ? m.roles : poolRoles);
      return { puuid: m.puuid, riotId: m.riotId, nickname: m.nickname, roles, profileIconId: resp?.profileIconId ?? undefined, isSelf: m.isSelf, included: activePuuids.includes(m.puuid), pool, mainRole: poolRoles[0] ?? null, inLobby: (ctx?.lobby ?? []).some(x => x.puuid === m.puuid), poolError: resp?.error ?? null } as TeamPlayer & { poolError?: string | null };
    });
  }, [rosterMembers, pools, activePuuids, ctx, overrides, champRoles]);
  const roleAssign = useMemo(() => assignRoles(teamBase.filter(t => t.included).map(t => ({ puuid: t.puuid, roles: t.roles ?? [] })), roleOverrides), [teamBase, roleOverrides]);
  const team: TeamPlayer[] = useMemo(() => teamBase.map(t => ({ ...t, assignedRole: roleAssign.assigned[t.puuid] ?? null })), [teamBase, roleAssign]);
  const tailored = isTailored(activePuuids.length);
  const cover = useMemo(() => new Map(VARIATIONS.map(v => [v.id, coverageFor(v, team)])), [team]);
  const hints = useMemo(() => tailored ? new Map(VARIATIONS.map(v => [v.id, subHintFor(v, team)])) : new Map(), [team, tailored]);
  const includedTeam = team.filter(p => p.included);
  const poolsLoaded = !loadingPools && includedTeam.some(p => p.pool != null);
  const canJudgeComps = poolsLoaded && includedTeam.length > 0;
  const isPlayableComp = (v: Variation) => canJudgeComps && compPlayable(cover.get(v.id)!, !!hints.get(v.id), includedTeam.length);
  const playableCount = VARIATIONS.filter(isPlayableComp).length;
  const saveRoster = (next: RosterMember[]) => { setRosterMembers(next); if (!isTauri()) return; setSaveError(""); void invoke<RosterMember[]>("comps_roster_save", { members: next }).then(saved => setRosterMembers(saved)).catch(() => setSaveError("Couldn't save your roster. Try again.")); };
  const applySwap = (outPuuid: string, inPuuid: string) => { setActivePuuids(prev => prev.map(id => id === outPuuid ? inPuuid : id)); setRoleOverrides(prev => { const n = { ...prev }; delete n[outPuuid]; return n; }); };
  const saveOverrides = (next: Context["overrides"]) => { setOverrides(next); const puuid = ctx?.me?.puuid; if (!puuid) return; setSaveError(""); overrideSaveChain.current = overrideSaveChain.current.then(() => invoke("comps_save_overrides", { ...next, expectedPuuid: puuid }).catch(() => setSaveError("Couldn't save pool changes. Try again."))); };

  useEffect(() => setSelected(null), [homeSignal]);
  if (selected) return <OpenVods.Provider value={onOpenVods ?? null}><div className="cp-page">
    <PageHeader title={<span className="cp-head"><button type="button" className="cp-head-back" onClick={() => setSelected(null)} aria-label="Back to all comps"><ChevronLeft size={18} aria-hidden />Comps</button><ChevronRight size={16} aria-hidden className="cp-head-sep" /><span>{selected.name}</span></span>} />
    <Detail variation={selected} cov={cover.get(selected.id)!} team={team} onOpenAnswers={() => { setSelected(null); setTab("answers"); }} /></div></OpenVods.Provider>;

  return <div className="cp-page">
    <PageHeader title="Comps" actions={<nav className="cp-tabs-main" role="tablist" aria-label="Comps sections">
      {([["comps", "Comps", VARIATIONS.length], ["answers", "Counters", ANSWERS.length], ["meta", "Pro meta", PRO_META.length]] as const).map(([id, label, n]) =>
        <button key={id} role="tab" aria-selected={tab === id} className={tab === id ? "on" : ""} onClick={() => setTab(id)}>{label}<em>{n}</em></button>)}
    </nav>} />
    <CompsTeamBar me={ctx?.me ?? null} roster={rosterMembers} lobby={ctx?.lobby ?? []} suggested={ctx?.suggested ?? []} team={team} active={activePuuids} onActiveChange={setActivePuuids} roleAssign={roleAssign} roleOverrides={roleOverrides} onRoleOverride={(puuid, role) => setRoleOverrides(prev => { const n = { ...prev }; if (role) n[puuid] = role; else delete n[puuid]; return n; })} onRosterChange={saveRoster} onOpenPools={() => setPoolsOpen(true)} teamLoading={teamLoading} signedIn={signedIn} />
    {saveError && <p className="cp-muted" role="alert">{saveError}</p>}
    {tab === "meta" ? <ProMeta onAnswers={() => setTab("answers")} /> : tab === "answers" ? <CompsAnswers team={team} onOpenVods={onOpenVods} /> : <>
      {canJudgeComps && <div className="f-row"><button type="button" className="f-switch" role="switch" aria-checked={hideRest} title="Ringed picks are champions your team plays" onClick={() => { setHideRest(!hideRest); setPeekLanes(new Set()); }}><i />{tailored ? "Only comps our five can play" : includedTeam.length === 1 && includedTeam[0].isSelf ? "Only comps I can play" : "Only comps we can play"}<em>{playableCount}</em></button></div>}
      {loadingPools && team.length > 0 && <Skeleton label="Loading champion pools" rows={2} height={56} />}
      <div className="cp-board">
        {STYLES.map(style => {
          const sorted = VARIATIONS.filter(v => v.style === style.name).sort((a, b) => (cover.get(b.id)?.count ?? 0) + (hints.get(b.id) ? .5 : 0) - (cover.get(a.id)?.count ?? 0) - (hints.get(a.id) ? .5 : 0));
          const { playable, rest } = canJudgeComps ? partitionPlayable(sorted, isPlayableComp) : { playable: sorted, rest: [] as Variation[] };
          const peek = peekLanes.has(style.name);
          const showRest = !hideRest || peek || !canJudgeComps;
          const vars = hideRest && !peek && canJudgeComps ? playable : [...playable, ...rest];
          // Hiding can shrink a column below its saved page; clamp instead of showing an empty page.
          const pages = Math.max(1, Math.ceil(vars.length / PAGE_SIZE)), page = Math.min(pageOf[style.name] ?? 0, pages - 1);
          const turn = (to: number) => setPageOf(prev => ({ ...prev, [style.name]: to }));
          const solo = !tailored && includedTeam.length === 1 && includedTeam[0].isSelf;
          const card = (v: Variation, dim: boolean) => { const c = cover.get(v.id)!; const hint = hints.get(v.id); const lead = v.slots.flatMap(x => x.options).find(o => o.core) ?? v.slots[2].options[0]; return <button key={v.id} className={`cp-var${dim ? " dim" : ""}`} onClick={() => setSelected(v)}>
              <span className="cp-var-art" aria-hidden="true"><ChampionArt id={lead.championId} name={lead.name} kind="splash" /></span>
              <span className="cp-var-h"><strong>{v.name}</strong>{tailored ? <span className={`cp-cv${c.count === 5 ? " full" : c.count === 4 ? " near" : ""}`}>{c.count}/5</span> : null}</span>
              <span className="cp-five">{ROLES.map(role => { const { champion, assignment } = slotPick(v, role, c); return <span key={role} className={!assignment ? (hint ? "sub" : undefined) : assignment.player.isSelf ? "lit me" : "lit"}><ChampionIcon id={champion.championId} name={champion.name} mode="desktop" tip={<ChampTip name={champion.name} player={assignment?.player} />} /></span>; })}</span>
              {hint && !dim ? <span className="cp-subhint" role="button" tabIndex={0} title="Click to make the swap" onClick={e => { e.stopPropagation(); applySwap(hint.out.puuid, hint.inn.puuid); }} onKeyDown={e => { if (e.key === "Enter") { e.stopPropagation(); applySwap(hint.out.puuid, hint.inn.puuid); } }}>⇄<span>Swap <b>{playerLabel(hint.out)}</b> → <b>{playerLabel(hint.inn)}</b> to play this</span></span> : <small>{solo ? soloNote(c) : peakLabel(v)}</small>}</button>; };
          return <div className="cp-lane" key={style.name} style={{ "--tone": style.tone } as React.CSSProperties}>
            <div className="cp-lane-h"><ChampionArt id={championId(style.art)} name={style.art} kind="splash" /><span className="cp-count">{hideRest && canJudgeComps && rest.length ? `${playable.length} of ${sorted.length} variations` : `${sorted.length} variations`}</span><h3>{style.name}</h3><p>{style.desc}</p></div>
            {hideRest && canJudgeComps && !peek && !playable.length && rest.length ? <div className="f-empty"><span><b>No {style.name} comp here.</b> None of the {rest.length} {tailored ? "is covered by your five" : includedTeam.length === 1 ? "uses a champion you play" : "uses a champion one of you plays"}.</span><button type="button" className="cp-btn" onClick={() => setPeekLanes(prev => new Set(prev).add(style.name))}>Show the {rest.length} anyway</button></div>
              : vars.slice(page * PAGE_SIZE, (page + 1) * PAGE_SIZE).map(v => card(v, canJudgeComps && showRest && rest.some(r => r.id === v.id)))}
            {hideRest && canJudgeComps && !peek && playable.length > 0 && rest.length > 0 && <button type="button" className="f-more" onClick={() => setPeekLanes(prev => new Set(prev).add(style.name))}>+{rest.length} hidden · show</button>}
            {!vars.length && !loadingPools && canJudgeComps && !rest.length && <p className="cp-muted">Nothing here.</p>}
            {pages > 1 && <nav className="cp-pager" aria-label={`${style.name} pages`}><button type="button" className="cp-btn" disabled={page === 0} onClick={() => turn(page - 1)} aria-label="Previous page"><ChevronLeft size={16} aria-hidden /></button><span>{page + 1} / {pages}</span><button type="button" className="cp-btn" disabled={page === pages - 1} onClick={() => turn(page + 1)} aria-label="Next page"><ChevronRight size={16} aria-hidden /></button></nav>}
          </div>;
        })}
      </div>
    </>}
    {poolsOpen && <PoolsDrawer team={team} champName={champName} champions={champions} champRoles={champRoles} overrides={overrides} saveOverrides={saveOverrides} onClose={() => setPoolsOpen(false)} />}
  </div>;
}

/** Pro meta: the most present champions on the latest pro patch, as ranked cards with pick/ban split and answers. */
function ProMeta({ onAnswers }: { onAnswers: () => void }) {
  return <section className="cp-meta" aria-label="Pro meta">
    <header className="cp-meta-head"><div><h2>Pro meta</h2><p>Most picked and banned champions in pro play · Patch {CURRENT_PATCH} · Worlds 2026, 94 games</p></div>
      <span className="cp-meta-key"><i className="pick" />Pick<i className="ban" />Ban</span></header>
    <div className="cp-meta-grid">{PRO_META.map((e, i) => <article key={e.champion} className="cp-meta-card ui-card">
      <div className="cp-meta-art"><ChampionArt id={championId(e.champion)} name={e.champion} kind="splash" /><span className="cp-meta-rank">{i + 1}</span><strong>{e.champion}</strong></div>
      <div className="cp-meta-body">
        <div className="cp-meta-title"><b>{e.presence}%</b><small>presence</small></div>
        <span className="cp-meta-split" role="img" aria-label={`${e.pickPct}% picked, ${e.banPct}% banned`}><i className="pick" style={{ width: `${e.pickPct}%` }} /><i className="ban" style={{ width: `${e.banPct}%` }} /></span>
        <span className="cp-meta-nums"><span>{e.pickPct}% pick</span><span>{e.banPct}% ban</span></span>
        {e.answers.length ? <button type="button" className="cp-meta-ans" onClick={onAnswers}><em>Countered by</em>{e.answers.map(n => <span key={n}><ChampionIcon id={championId(n)} name={n} mode="desktop" />{n}</span>)}</button>
          : <small className="cp-muted">No clear counter yet</small>}
      </div></article>)}</div>
    <p className="cp-muted">Source: Leaguepedia, researched 2026-11-14.</p>
  </section>;
}

const ORDER_CLASS: Record<CompSlot["draftOrder"], string> = { "Pick early": "early", "Hold for last": "hold", "Pick together": "together", Flexible: "flex" };
const PLAN = [["When you're strong", "win", "▲"], ["How fights start", "start", "⚑"], ["How you lose", "lose", "✕"]] as const;

function Detail({ variation: v, cov, team, onOpenAnswers }: { variation: Variation; cov: Coverage; team: TeamPlayer[]; onOpenAnswers: () => void }) {
  const cores = v.slots.flatMap(s => s.options.filter(o => o.core));
  const art = (cores.length ? cores : [v.slots[2].options[0]]).slice(0, 2);
  // What beats this comp: answers drafted into its core picks. Fillers and the comp's own answers are noise here.
  const core = new Set(cores.map(o => o.name));
  const linked = ANSWERS.filter(a => a.threat.some(n => core.has(n))).sort((a, b) => b.games - a.games);
  const shownLinked = linked.slice(0, 4);
  const peak = Math.max(v.power.early, v.power.mid, v.power.late);
  const browse = teamMode(team) !== "team";
  // Which alternative is shown per flexible role; core roles never change.
  const [swap, setSwap] = useState<Partial<Record<Role, number>>>({});
  const playerFor = (id: number) => team.find(p => p.included && p.pool?.some(e => e.championId === id && !e.hidden));
  return <>
    <section className="cp-hero ui-card" style={{ "--tone": STYLES.find(st => st.name === v.style)?.tone ?? "var(--ui-brand)" } as React.CSSProperties}>
      <div className={`cp-hero-art${art.length > 1 ? " duo" : ""}`}>{art.map(o => <ChampionArt key={o.championId} id={o.championId} name={o.name} kind="splash" />)}</div>
      <div className="cp-hero-copy">
        <h2>{v.name}</h2><p>{v.winCondition}</p>
        <div className="cp-tags"><span className="cp-pill lime">{v.style}</span>{v.tags.map(t => <span key={t} className="cp-pill">{t}</span>)}{!browse && <span className={`cp-pill ${cov.count === 5 ? "lime" : "gold"}`}>{cov.count} / 5 on your team</span>}</div>
      </div>
      <div className="cp-curve" role="img" aria-label={`Power curve: ${peakLabel(v)}`}><header>Power curve <b>{peakLabel(v)}</b></header>
        <div className="cp-bars">{[v.power.early, v.power.mid, v.power.late].map((val, i) => <i key={i} className={val === peak ? "peak" : undefined} style={{ height: `${Math.max(10, val)}%` }} />)}</div>
        <div className="cp-bars-l"><span>Early</span><span>Mid</span><span>Late</span></div></div>
    </section>
    <section className="cp-lineup" aria-label="Who plays what">
      {v.slots.map((slot, i) => {
        const a = cov.assignments.find(x => x.role === slot.role);
        const isCore = slot.options.some(o => o.core);
        const shown = slot.options.find(o => o.championId === swap[slot.role]) ?? slotPick(v, slot.role, cov).champion;
        const who = a?.champion?.championId === shown.championId ? a.player : playerFor(shown.championId);
        const entry = who?.pool?.find(e => e.championId === shown.championId);
        const alts = isCore ? [] : slot.options.filter(o => o.championId !== shown.championId);
        const pairNext = slot.draftOrder === "Pick together" && v.slots[i + 1]?.draftOrder === "Pick together";
        return <div key={slot.role} className="cp-pick-wrap">
          <article className={`cp-pick${who ? " lit" : ""}`}>
            <div className="cp-portrait"><ChampionArt id={shown.championId} name={shown.name} kind="loading" />
              <span className="cp-pick-top"><span className="cp-role">{ROLE_LABEL[slot.role]}</span>{isCore && <span className="cp-core">★ CORE</span>}</span>
              <strong>{shown.name}</strong></div>
            <div className="cp-pick-body">
              <span className={`cp-who${who ? " on" : ""}`}>{who ? <>{who.profileIconId != null && <img src={profileIcon(who.profileIconId)} alt="" />}<span><b>{who.isSelf ? "You" : shortName(who)}</b>{entry?.reason && <small title={entry.reason}>{entry.reason}</small>}</span></> : browse ? (isCore ? "Keep this pick" : "Any of these work") : "Nobody plays this yet"}</span>
              <span className={`cp-order ${ORDER_CLASS[slot.draftOrder]}`}>{slot.draftOrder}</span>
              {alts.length > 0 && <div className="cp-swap" role="group" aria-label={`${ROLE_LABEL[slot.role]} alternatives`}><em>Or swap in</em><span>
                {alts.map(o => <button key={o.championId} type="button" className={playerFor(o.championId) ? "lit" : undefined} aria-label={`Swap in ${o.name}`}
                  onClick={() => setSwap(prev => ({ ...prev, [slot.role]: o.championId }))}><ChampionIcon id={o.championId} name={o.name} mode="desktop" tip={<ChampTip name={o.name} player={playerFor(o.championId)} />} /></button>)}</span></div>}
            </div>
          </article>
          {pairNext && <span className="cp-pair" title="Picked in the same draft turn" aria-hidden="true">⛓</span>}
        </div>;
      })}
    </section>
    <div className="cp-cols">
      <section className="ui-card cp-panel"><header><h3>Game plan</h3></header>
        <div className="cp-plan">{v.tips.map((t, i) => { const [label, tone, mark] = PLAN[i] ?? ["Tip", "start", "•"]; return <div key={i} className={`cp-step ${tone}`}><i aria-hidden="true">{mark}</i><div><strong>{label}</strong><p>{t}</p></div></div>; })}</div></section>
      <section className="ui-card cp-panel"><header><h3>Watch out for</h3><span className="cp-weak">{v.weakAgainst.map(w => <button key={w} className="cp-pill btn loss" onClick={onOpenAnswers}>{w}</button>)}</span></header>
        {shownLinked.map(a => <div key={a.id} className="cp-threat"><span className="cp-ics">{a.answer.map(n => <ChampionIcon key={n} id={championId(n)} name={n} mode="desktop" />)}</span><p><b>{a.answer.join(" + ")}</b>{a.why}</p></div>)}
        {linked.length > shownLinked.length && <button className="cp-link" onClick={onOpenAnswers}>See all {linked.length} counters ›</button>}
        {!linked.length && <p className="cp-muted">No pro counters to its core picks in the data.</p>}</section>
    </div>
    <section className="ui-card cp-panel"><header><h3>In pro play</h3><small>Real games · Leaguepedia</small></header>
      {v.proExamples.length ? <div className="cp-games">{v.proExamples.map(g => <ProGame key={`${g.title}|${g.detail}`} game={g} />)}</div> : <p className="cp-muted">No example games yet.</p>}
    </section></>;
}

function PoolsDrawer({ team, champName, champions, champRoles, overrides, saveOverrides, onClose }: { team: TeamPlayer[]; champName: Map<number, string>; champions: ChampionOption[]; champRoles: Map<number, Role>; overrides: Context["overrides"]; saveOverrides: (o: Context["overrides"]) => void; onClose: () => void }) {
  const [tabPuuid, setTabPuuid] = useState(team.find(p => p.isSelf)?.puuid ?? team[0]?.puuid ?? "");
  const closeRef = useRef<HTMLButtonElement>(null);
  useEffect(() => { closeRef.current?.focus(); const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") onClose(); }; window.addEventListener("keydown", onKey); return () => window.removeEventListener("keydown", onKey); }, [onClose]);
  const player = team.find(p => p.puuid === tabPuuid) ?? team[0];
  const grouped: { role: Role | null; entries: PoolEntry[] }[] = ROLES.map(role => ({ role, entries: (player?.pool ?? []).filter(e => e.role === role) })).filter(g => g.entries.length);
  const unroled = (player?.pool ?? []).filter(e => !e.role || !ROLES.includes(e.role));
  if (unroled.length) grouped.push({ role: null, entries: unroled });
  return <div className="cp-drawer-wrap" onClick={e => { if (e.target === e.currentTarget) onClose(); }}><aside className="cp-drawer" role="dialog" aria-modal="true" aria-label="Team pools">
    <header><h2>Team pools</h2><span className="cp-pill lime">Auto</span><button ref={closeRef} className="cp-btn" onClick={onClose} aria-label="Close pools">✕</button></header>
    <p>Built from each player's last 20 games and champion mastery. Your own pool is the only one you can change; teammates update by themselves.</p>
    <div className="cp-tabs">{team.map(p => <button key={p.puuid} className={p.puuid === player?.puuid ? "on" : ""} onClick={() => setTabPuuid(p.puuid)}>{p.profileIconId != null && <img className="cp-tab-icon" src={profileIcon(p.profileIconId)} alt="" />}{p.isSelf ? "You" : shortName(p)}</button>)}</div>
    {!player ? <p className="cp-muted">No players yet.</p> : (player as TeamPlayer & { poolError?: string | null }).poolError && !player.pool ? <Notice tone="warn">Pool unavailable for this player. The rest of the team still works.</Notice> : <>
      {grouped.map(g => <div key={g.role ?? "other"}><div className="cp-role-h"><span className="cp-role">{g.role ? ROLE_LABEL[g.role] : "?"}</span><small>{!g.role ? "Role unknown" : player.mainRole === g.role ? "Main role" : "Other role"}</small></div>
        <div className="cp-pool">{g.entries.map(e => <div key={e.championId} className={`cp-pc${e.added ? " added" : ""}${e.hidden ? " hidden" : ""}`}>
          <ChampionIcon id={e.championId} name={champName.get(e.championId)} mode="desktop" /><span><strong>{champName.get(e.championId) ?? `#${e.championId}`}</strong><small>{e.added ? "Added by you" : e.reason}</small></span>{e.masteryLevel ? <img className="cp-mastery" src={masteryCrest(e.masteryLevel)} alt={`Mastery ${e.masteryLevel}`} title={`Mastery level ${e.masteryLevel}${e.masteryPoints ? ` · ${Math.round(e.masteryPoints / 1000)}k points` : ""}`} /> : null}
          {player.isSelf && (e.hidden ? <button className="cp-link" onClick={() => saveOverrides({ ...overrides, hidden: overrides.hidden.filter(id => id !== e.championId) })}>Show</button>
            : <button className="cp-link" aria-label={`Hide ${champName.get(e.championId)}`} onClick={() => saveOverrides(e.added ? { ...overrides, added: overrides.added.filter(a => a.championId !== e.championId) } : { ...overrides, hidden: [...overrides.hidden, e.championId] })}>✕</button>)}
        </div>)}</div></div>)}
      {player.isSelf && <div className="cp-add"><ChampionPicker label="Add a champion" value={0} champions={champions} onChange={id => { if (id && !overrides.added.some(a => a.championId === id)) saveOverrides({ ...overrides, added: [...overrides.added, { championId: id, role: addedChampionRole(champRoles.get(id), player.mainRole) }], hidden: overrides.hidden.filter(x => x !== id) }); }} /></div>}
      <div className="cp-foot"><span>{player.isSelf ? "Only your pool is editable." : "Read-only · automatic"}</span>{player.isSelf && <button className="cp-btn" onClick={() => saveOverrides({ added: [], hidden: [] })}>Reset to automatic</button>}</div>
    </>}
  </aside></div>;
}

/** Solo card line when the team is only you: the champion you play in this comp, if any. */
function soloNote(cov: Coverage): string {
  const mine = cov.assignments.find(a => a.champion && a.player?.isSelf);
  return mine?.champion ? `You play ${mine.champion.name}` : "You don't play this yet";
}

/** The champion a comp slot shows: the one your team plays when someone does, else the comp's core (or first) option. */
function slotPick(v: Variation, role: Role, cov: Coverage) {
  const assignment = cov.assignments.find(a => a.role === role && a.champion) as Extract<Coverage["assignments"][number], { champion: CompOption }> | undefined;
  const slot = v.slots.find(o => o.role === role)!;
  return { champion: assignment?.champion ?? slot.options.find(o => o.core) ?? slot.options[0], assignment };
}

/** A pro game as a card; with a VOD, the thumbnail opens it on the VODs page in the full player. */
function ProGame({ game: g }: { game: Variation["proExamples"][number] }) {
  const open = useContext(OpenVods);
  const meta = <span className="cp-game-meta"><strong>{g.title}</strong><small>{g.result && <span className={g.result === "Win" ? "cp-win" : "cp-loss"}>{g.result}</span>} · {g.detail}</small></span>;
  if (!g.vod || !open) return <div className="cp-game">{meta}</div>;
  const play = { videoId: g.vod, title: g.title, rankClaim: "Pro play", patch: CURRENT_PATCH, durationSec: 0, startSec: g.vodStart, moments: [] };
  return <button type="button" className="cp-game cp-vod-play" aria-label={`Watch ${g.title}`} onClick={() => open({ champion: null, play })}>
    <span className="cp-thumb"><img src={`https://i.ytimg.com/vi/${g.vod}/mqdefault.jpg`} alt="" loading="lazy" /><span aria-hidden="true">▶</span></span>{meta}</button>;
}
