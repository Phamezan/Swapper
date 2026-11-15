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
import { addedChampionRole, applyOverrides, coverageFor, isOlderMeta, peakLabel, selectTeamMembers, teamMode, ROLES, ROLE_LABEL, type CompOption, type CompSlot, type Coverage, type PoolEntry, type Role, type TeamPlayer, type Variation } from "./compsModel";
import type { Snapshot } from "./types";
import "./comps.css";

type Member = { puuid: string; riotId: string; games?: number };
type Context = { me: Member | null; lobby: Member[]; saved: Member[]; suggested: { puuid: string; riotId: string; games: number }[]; overrides: { added: { championId: number; role: string }[]; hidden: number[] } };
type PoolResp = { puuid: string; entries: PoolEntry[]; error: string | null; profileIconId?: number | null };
const CDRAGON = "https://raw.communitydragon.org/latest/plugins/rcp-fe-lol-shared-components/global/default";
/** Mastery crest art from CommunityDragon; levels above 10 reuse the level-10 crest. */
const masteryCrest = (level: number) => `${CDRAGON}/mastery-${Math.min(10, Math.max(1, level))}.png`;
const profileIcon = (id: number) => `https://cdn.communitydragon.org/latest/profile-icon/${id}`;
const displayName = (m: Member, me: Member | null) => me && m.puuid === me.puuid ? "You" : shortName(m);
/** A player's name without the tag; "Player" when the client gave no name (never the raw PUUID). */
const shortName = (p: { riotId: string; puuid: string }) => (p.riotId && p.riotId !== p.puuid ? p.riotId.split("#")[0] : "Player");

const OpenVods = createContext<((request: VodRequest) => void) | null>(null);

/** Champion tooltip: its name, plus the icon and name of the teammate who plays it. */
function ChampTip({ name, player }: { name: string; player?: TeamPlayer | null }) {
  return <span className="cp-tip"><b>{name}</b>{player && <span className="cp-tip-who">{player.profileIconId != null && <img src={profileIcon(player.profileIconId)} alt="" />}{player.isSelf ? "You" : shortName(player)}</span>}</span>;
}

/** Comps per column page on the overview. */
const PAGE_SIZE = 5;

/** Fewest comps a filter must leave before its button is shown. */
const MIN_FILTER_RESULTS = 3;

export function Comps({ snapshot, champions, signedIn, onOpenVods, homeSignal }: { snapshot: Snapshot | null; champions: ChampionOption[]; signedIn: boolean; onOpenVods?: (request: VodRequest) => void; homeSignal?: number }) {
  const [tab, setTab] = useState<"comps" | "answers" | "meta">("comps");
  const [filter, setFilter] = useState<"all" | "playable" | "byme">("all");
  const [answerFilter, setAnswerFilter] = useState<"ours" | "all">("ours");
  const [selected, setSelected] = useState<Variation | null>(null);
  const [poolsOpen, setPoolsOpen] = useState(false);
  const [ctx, setCtx] = useState<Context | null>(null);
  const [roster, setRoster] = useState<Member[]>([]);
  const [excluded, setExcluded] = useState<Set<string>>(new Set());
  const [pools, setPools] = useState<Map<string, PoolResp>>(new Map());
  const [loadingPools, setLoadingPools] = useState(false);
  const [source, setSource] = useState<"lobby" | "saved" | "auto">("auto");
  const [added, setAdded] = useState<Member[]>([]);
  const [addOpen, setAddOpen] = useState(false);
  const [editing, setEditing] = useState(false);
  // Players removed while editing; they stay out of the roster until the page reloads or the team is saved without them.
  const [removed, setRemoved] = useState<Set<string>>(new Set());
  const [addText, setAddText] = useState("");
  const [addError, setAddError] = useState("");
  const [pageOf, setPageOf] = useState<Record<string, number>>({});
  const [overrides, setOverrides] = useState<Context["overrides"]>({ added: [], hidden: [] });
  const [teamLoading, setTeamLoading] = useState(isTauri());
  const [saveError, setSaveError] = useState("");
  const overrideSaveChain = useRef<Promise<unknown>>(Promise.resolve());
  const forcedByMe = useRef(false);
  const answerDefaulted = useRef(false);
  const accountKey = snapshot?.summoner.riotId ?? "";

  useEffect(() => {
    if (!isTauri()) { setTeamLoading(false); return; }
    let live = true;
    const load = () => { setTeamLoading(true); return invoke<Context>("comps_team").then(c => { if (live) { setCtx(c); setOverrides(c.overrides); } }).catch(() => { if (live) setCtx({ me: null, lobby: [], saved: [], suggested: [], overrides: { added: [], hidden: [] } }); }).finally(() => { if (live) setTeamLoading(false); }); };
    void load();
    const onFocus = () => { if (document.visibilityState === "visible") void load(); };
    window.addEventListener("focus", onFocus); document.addEventListener("visibilitychange", onFocus);
    return () => { live = false; window.removeEventListener("focus", onFocus); document.removeEventListener("visibilitychange", onFocus); };
  }, [signedIn, accountKey]);
  // Roster: party lobby > saved team > me + season suggestions (backend), snapshot fallback.
  useEffect(() => {
    if (!ctx) return;
    const keep = <T extends { puuid: string }>(list: T[]) => list.filter(m => !removed.has(m.puuid));
    const { members, source: src } = selectTeamMembers({ lobby: keep(ctx.lobby), saved: keep(ctx.saved), me: ctx.me, suggested: keep(ctx.suggested ?? []), snapshotGames: snapshot?.games ?? [], added: keep(added) });
    setSource(src);
    setRoster(prev => prev.length === members.length && prev.every((m, i) => m.puuid === members[i].puuid) ? prev : members);
  }, [ctx, snapshot, added, removed]);

  useEffect(() => {
    if (!roster.length || !isTauri()) return;
    let live = true; setLoadingPools(true);
    void invoke<PoolResp[]>("comps_pools", { puuids: roster.map(m => m.puuid) }).then(rows => { if (live) setPools(new Map(rows.map(r => [r.puuid, r]))); })
      .catch(() => { if (live) setPools(new Map(roster.map(m => [m.puuid, { puuid: m.puuid, entries: [], error: "Pool unavailable" }]))); })
      .finally(() => { if (live) setLoadingPools(false); });
    return () => { live = false; };
  }, [roster]);

  const [champRoles, setChampRoles] = useState<Map<number, Role>>(new Map());
  useEffect(() => {
    if (!isTauri()) return;
    let live = true;
    void Promise.all(TIER_ROLES.map(r => invoke<TierListView>("get_tier_list", { position: r, tier: "emerald_plus" }).then(v => [r, v.rows] as const).catch(() => [r, []] as const)))
      .then(entries => { if (live) setChampRoles(new Map([...mainRoles(Object.fromEntries(entries))].map(([id, r]) => [id, (r === "adc" ? "bot" : r) as Role]))); });
    return () => { live = false; };
  }, []);
  const champName = useMemo(() => { const m = new Map(champions.map(c => [c.id, c.name])); if (snapshot) for (const [id, n] of Object.entries(snapshot.championNames)) m.set(Number(id), n); return m; }, [champions, snapshot]);
  const team: TeamPlayer[] = useMemo(() => roster.map(m => {
    const resp = pools.get(m.puuid);
    let pool: PoolEntry[] | null = resp ? (resp.error && !resp.entries.length ? null : resp.entries.map(e => e.role ? e : { ...e, role: champRoles.get(e.championId) ?? e.role })) : null;
    const isMe = Boolean(ctx?.me && m.puuid === ctx.me.puuid);
    if (isMe && pool) pool = applyOverrides(pool, overrides.added.map(a => ({ championId: a.championId, role: (a.role || "mid") as Role })), overrides.hidden);
    const roleCounts = new Map<string, number>();
    for (const e of pool ?? []) if (e.role) roleCounts.set(e.role, (roleCounts.get(e.role) ?? 0) + (e.games ?? 1));
    const mainRole = [...roleCounts.entries()].sort((a, b) => b[1] - a[1])[0]?.[0] as Role | undefined;
    return { puuid: m.puuid, riotId: m.riotId, games: m.games, profileIconId: resp?.profileIconId ?? undefined, isSelf: isMe, included: !excluded.has(m.puuid), pool, mainRole: mainRole ?? null, poolError: resp?.error ?? null } as TeamPlayer & { poolError?: string | null };
  }), [roster, pools, excluded, ctx, overrides, champRoles]);

  const cover = useMemo(() => new Map(VARIATIONS.map(v => [v.id, coverageFor(v, team)])), [team]);
  const playableCount = VARIATIONS.filter(v => cover.get(v.id)?.count === 5).length;
  const byMeCount = VARIATIONS.filter(v => cover.get(v.id)?.playableByMe).length;
  const savedKey = (ctx?.saved ?? []).map(m => m.puuid).sort().join(",");
  const rosterKey = roster.filter(m => !excluded.has(m.puuid)).map(m => m.puuid).sort().join(",");
  const showSaveStrip = Boolean(ctx?.me) && roster.length >= 2 && rosterKey !== savedKey;
  const includedTeam = team.filter(p => p.included);
  const hasMates = includedTeam.some(p => !p.isSelf);
  // "Playable by us" has no button without teammates; fall back to All instead of an empty board.
  const poolsLoaded = !loadingPools && includedTeam.some(p => p.pool != null);
  // A filter is only offered when it leaves a useful board; one that would show a handful of comps falls back to All.
  const showPlayable = hasMates && playableCount >= MIN_FILTER_RESULTS, showByMe = byMeCount >= MIN_FILTER_RESULTS;
  const shownFilter = (filter === "playable" && !showPlayable) || (filter === "byme" && !showByMe && poolsLoaded) ? "all" : filter;
  useEffect(() => { if (!forcedByMe.current && poolsLoaded && showByMe && includedTeam.length === 1 && includedTeam[0].isSelf) { forcedByMe.current = true; setFilter("byme"); } }, [poolsLoaded, showByMe, includedTeam]);
  useEffect(() => { if (!answerDefaulted.current && !loadingPools && team.length && team.every(p => pools.has(p.puuid)) && !team.some(p => p.included && p.pool?.length)) { answerDefaulted.current = true; setAnswerFilter("all"); } }, [loadingPools, team, pools]);
  const saveTeam = () => { const members = roster.filter(m => !excluded.has(m.puuid)); const puuid = ctx?.me?.puuid; if (!puuid) return; setSaveError(""); void invoke("comps_save_team", { members, expectedPuuid: puuid }).then(() => setCtx(c => c ? { ...c, saved: members } : c)).catch((e: unknown) => setSaveError(`Could not save team: ${String(e)}`)); };
  const addPlayer = () => { const riotId = addText.trim(); if (!riotId) return; setAddError("");
    void invoke<Member>("comps_resolve", { riotId }).then(m => { setAdded(prev => [...prev.filter(a => a.puuid !== m.puuid), m]); setAddText(""); setAddOpen(false); })
      .catch(() => setAddError("No player found for that Riot ID.")); };
  const saveOverrides = (next: Context["overrides"]) => { setOverrides(next); const puuid = ctx?.me?.puuid; if (!puuid) return; setSaveError(""); overrideSaveChain.current = overrideSaveChain.current.then(() => invoke("comps_save_overrides", { ...next, expectedPuuid: puuid }).catch((e: unknown) => setSaveError(`Could not save pool changes: ${String(e)}`))); };

  useEffect(() => setSelected(null), [homeSignal]);
  if (selected) return <OpenVods.Provider value={onOpenVods ?? null}><div className="cp-page">
    <PageHeader title={<span className="cp-head"><button type="button" className="cp-head-back" onClick={() => setSelected(null)} aria-label="Back to all comps"><ChevronLeft size={18} aria-hidden />Comps</button><ChevronRight size={16} aria-hidden className="cp-head-sep" /><span>{selected.name}</span></span>} />
    <Detail variation={selected} cov={cover.get(selected.id)!} team={team} onOpenAnswers={() => { setSelected(null); setTab("answers"); }} /></div></OpenVods.Provider>;

  return <div className="cp-page">
    <PageHeader title="Comps" />
    <nav className="cp-tabs-main" role="tablist" aria-label="Comps sections">
      {([["comps", "Comps", VARIATIONS.length], ["answers", "Answers", ANSWERS.length], ["meta", "Pro meta", PRO_META.length]] as const).map(([id, label, n]) =>
        <button key={id} role="tab" aria-selected={tab === id} className={tab === id ? "on" : ""} onClick={() => setTab(id)}>{label}<em>{n}</em></button>)}
    </nav>
    <section className="cp-teambar ui-card">
      <span className="cp-eyebrow">Your team</span>
      {teamLoading && team.length === 0 ? <span className="cp-muted">Loading your team…</span> : team.length === 0 && <span className="cp-muted">{signedIn ? "No team yet. Play Flex or Clash with the same people, or add a player." : "Sign in to League Client to load your team and champion pools."}</span>}
      {team.map(p => <span key={p.puuid} className="cp-tm-wrap"><button className={`cp-tm${p.included ? "" : " off"}`} aria-pressed={p.included} onClick={() => setExcluded(prev => { const n = new Set(prev); if (n.has(p.puuid)) n.delete(p.puuid); else n.add(p.puuid); return n; })}>
        <ChampionIcon id={p.pool?.[0]?.championId ?? -1} name={p.pool?.[0] ? champName.get(p.pool[0].championId) : displayName(p, ctx?.me ?? null)} mode="desktop" />
        <span>{p.isSelf ? "You" : displayName(p, ctx?.me ?? null)}</span>{p.games ? <small className="cp-withyou" title={`Played ${p.games} Flex or Clash games with you`}>{p.games}×</small> : null}{p.mainRole && <span className="cp-role">{ROLE_LABEL[p.mainRole]}</span>}
      </button>{editing && !p.isSelf && <button type="button" className="cp-tm-remove" aria-label={`Remove ${displayName(p, ctx?.me ?? null)} from the team`}
        onClick={() => { setRemoved(prev => new Set(prev).add(p.puuid)); setAdded(prev => prev.filter(a => a.puuid !== p.puuid)); }}>✕</button>}</span>)}
      {(editing || source !== "lobby") && roster.length < 5 && !addOpen && <button className="cp-btn" onClick={() => setAddOpen(true)}>Add player</button>}
      {editing && <button className="cp-btn primary" onClick={() => { setEditing(false); setAddOpen(false); }}>Done</button>}
      <div className="cp-tools">{!editing && ctx?.me && <button className="cp-btn" onClick={() => { setEditing(true); setAddOpen(roster.length < 5); }}>Edit team</button>}<button className="cp-btn" onClick={() => setPoolsOpen(true)}>Pools</button></div>
      {addOpen && <span className="cp-addplayer"><input aria-label="Riot ID" placeholder="Name#TAG" value={addText} onChange={e => setAddText(e.target.value)} onKeyDown={e => { if (e.key === "Enter") addPlayer(); }} /><button className="cp-btn primary" onClick={addPlayer}>Add</button>{addError && <small role="alert">{addError}</small>}</span>}
    </section>
    {saveError && <p className="cp-muted" role="alert">{saveError}</p>}
    {showSaveStrip && !editing && <div className="cp-savestrip"><span>Is this your team?</span><button className="cp-btn primary" onClick={saveTeam}>Save</button><button className="cp-btn" onClick={() => { setEditing(true); setAddOpen(roster.length < 5); }}>Edit</button></div>}
    {tab === "meta" ? <ProMeta onAnswers={() => setTab("answers")} /> : tab === "answers" ? <Answers team={team} filter={answerFilter} setFilter={setAnswerFilter} /> : <>
      <div className="cp-filters">
        {showPlayable || showByMe ? <div className="cp-toggles" role="group" aria-label="Filter comps"><button aria-pressed={shownFilter === "all"} onClick={() => setFilter("all")}>All</button>{showPlayable && <button aria-pressed={shownFilter === "playable"} onClick={() => setFilter("playable")}>Playable by us <em>{playableCount}</em></button>}{showByMe && <button aria-pressed={shownFilter === "byme"} onClick={() => setFilter("byme")}>By me <em>{byMeCount}</em></button>}</div> : null}
      </div>
      {loadingPools && team.length > 0 && <Skeleton label="Loading champion pools" rows={2} height={56} />}
      <div className="cp-board">
        {STYLES.map(style => {
          const vars = VARIATIONS.filter(v => v.style === style.name).filter(v => shownFilter === "all" || (shownFilter === "playable" ? cover.get(v.id)?.count === 5 : cover.get(v.id)?.playableByMe)).sort((a, b) => (cover.get(b.id)?.count ?? 0) - (cover.get(a.id)?.count ?? 0));
          // A filter can shrink a column below its saved page; clamp instead of showing an empty page.
          const pages = Math.max(1, Math.ceil(vars.length / PAGE_SIZE)), page = Math.min(pageOf[style.name] ?? 0, pages - 1);
          const turn = (to: number) => setPageOf(prev => ({ ...prev, [style.name]: to }));
          return <div className="cp-lane" key={style.name}>
            <div className="cp-lane-h" style={{ "--tone": style.tone } as React.CSSProperties}><ChampionArt id={championId(style.art)} name={style.art} kind="splash" /><span className="cp-count">{VARIATIONS.filter(v => v.style === style.name).length} variations</span><h3>{style.name}</h3><p>{style.desc}</p></div>
            {vars.slice(page * PAGE_SIZE, (page + 1) * PAGE_SIZE).map(v => { const c = cover.get(v.id)!; const mode = teamMode(team); const solo = mode === "solo" ? soloBadge(c) : null; const browse = mode !== "team"; return <button key={v.id} className="cp-var" onClick={() => setSelected(v)}>
              <span className="cp-var-h"><strong>{v.name}</strong>{solo ? (solo.role && <span className="cp-cv full">{solo.role}</span>) : browse ? null : <span className={`cp-cv${c.count === 5 ? " full" : c.count === 4 ? " near" : ""}`}>{c.count}/5</span>}</span>
              <span className="cp-five">{ROLES.map(role => { const { champion, assignment } = slotPick(v, role, c); return <span key={role} className={!assignment ? undefined : assignment.player.isSelf ? "lit me" : "lit"}><ChampionIcon id={champion.championId} name={champion.name} mode="desktop" tip={<ChampTip name={champion.name} player={assignment?.player} />} /></span>; })}</span>
              <small>{solo?.role ? solo.note : peakLabel(v)}</small></button>; })}
            {!vars.length && <p className="cp-muted">{loadingPools ? "Loading pools…" : "Nothing here for this filter."}</p>}
            {pages > 1 && <nav className="cp-pager" aria-label={`${style.name} pages`}><button type="button" className="cp-btn" disabled={page === 0} onClick={() => turn(page - 1)} aria-label="Previous page"><ChevronLeft size={16} aria-hidden /></button><span>{page + 1} / {pages}</span><button type="button" className="cp-btn" disabled={page === pages - 1} onClick={() => turn(page + 1)} aria-label="Next page"><ChevronRight size={16} aria-hidden /></button></nav>}
          </div>;
        })}
      </div>
    </>}
    {poolsOpen && <PoolsDrawer team={team} champName={champName} champions={champions} champRoles={champRoles} overrides={overrides} saveOverrides={saveOverrides} onClose={() => setPoolsOpen(false)} />}
  </div>;
}

function Answers({ team, filter, setFilter }: { team: TeamPlayer[]; filter: "ours" | "all"; setFilter: (f: "ours" | "all") => void }) {
  const can = (a: (typeof ANSWERS)[number]) => a.answer.flatMap(name => { const id = championId(name); return team.filter(p => p.included).filter(p => (p.pool ?? []).some(e => e.championId === id && !e.hidden)).map(p => `${p.isSelf ? "You" : shortName(p)} · ${name}`); });
  const rows = ANSWERS.filter(a => filter === "all" || can(a).length);
  return <><div className="cp-filters"><div className="cp-toggles" role="group" aria-label="Filter answers"><button aria-pressed={filter === "ours"} onClick={() => setFilter("ours")}>Our pools</button><button aria-pressed={filter === "all"} onClick={() => setFilter("all")}>All</button></div></div>
    <div className="cp-answers">{rows.map(a => { const mine = can(a); const old = isOlderMeta(a.patch, CURRENT_PATCH); return <article key={a.id} className={`cp-answer ui-card${old ? " old" : ""}`}>
      <span className="cp-duo"><span className="cp-ics big">{a.threat.map(n => <ChampionIcon key={n} id={championId(n)} name={n} mode="desktop" />)}</span><span><strong>{a.threat.join(" + ")}</strong><small>Pro pick</small></span></span><span className="cp-arrow">→</span>
      <span className="cp-duo"><span className="cp-ics big">{a.answer.map(n => <ChampionIcon key={n} id={championId(n)} name={n} mode="desktop" />)}</span><span><strong>{a.answer.join(" + ")}</strong><small>Answer</small></span></span>
      <span className="cp-answer-copy"><span className={`cp-kind ${a.kind}`}>{a.kind === "draft" ? "Draft answer" : "Lane counter"}</span><small>{a.reason}</small>
      <small>Patch {a.patch} · {a.games} games{a.kind === "draft" && a.lift ? ` · picked ${a.lift}× more often after ${a.threat.join(" + ")}` : ""}{old ? " · older meta" : ""}</small></span>
      <span className="cp-can">{mine.length ? <><span>Your team can</span><b>{mine.join(", ")}</b></> : <span className="no">Nobody plays {a.answer.join(" or ")}</span>}</span></article>; })}
    {!rows.length && <p className="cp-muted">No answers in your team's pools yet.</p>}</div></>;
}

/** Pro meta: the most present champions on the latest pro patch, as ranked cards with pick/ban split and answers. */
function ProMeta({ onAnswers }: { onAnswers: () => void }) {
  return <section className="cp-meta" aria-label="Pro meta">
    <header className="cp-meta-head"><div><h2>Pro meta</h2><p>Most picked and banned champions in pro play · Patch {CURRENT_PATCH} · Worlds 2026, 94 games</p></div>
      <span className="cp-meta-key"><i className="pick" />Pick<i className="ban" />Ban</span></header>
    <div className="cp-meta-grid">{PRO_META.map((e, i) => <article key={e.champion} className="cp-meta-card ui-card">
      <div className="cp-meta-art"><ChampionArt id={championId(e.champion)} name={e.champion} kind="splash" /><span className="cp-meta-rank">#{i + 1}</span></div>
      <div className="cp-meta-body">
        <div className="cp-meta-title"><strong>{e.champion}</strong><b>{e.presence}%</b></div>
        <small className="cp-muted">presence</small>
        <span className="cp-meta-split" role="img" aria-label={`${e.pickPct}% picked, ${e.banPct}% banned`}><i className="pick" style={{ width: `${e.pickPct}%` }} /><i className="ban" style={{ width: `${e.banPct}%` }} /></span>
        <span className="cp-meta-nums"><span>{e.pickPct}% pick</span><span>{e.banPct}% ban</span></span>
        {e.answers.length ? <button type="button" className="cp-meta-ans" onClick={onAnswers}><em>Answered by</em>{e.answers.map(n => <span key={n}><ChampionIcon id={championId(n)} name={n} mode="desktop" />{n}</span>)}</button>
          : <small className="cp-muted">No clear answer yet</small>}
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
    <section className="cp-hero ui-card">
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
        {shownLinked.map(a => <div key={a.id} className="cp-threat"><span className="cp-ics">{a.answer.map(n => <ChampionIcon key={n} id={championId(n)} name={n} mode="desktop" />)}</span><p><b>{a.answer.join(" + ")}</b>{a.reason}</p></div>)}
        {linked.length > shownLinked.length && <button className="cp-link" onClick={onOpenAnswers}>See all {linked.length} answers ›</button>}
        {!linked.length && <p className="cp-muted">No pro answers to its core picks in the data.</p>}</section>
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

/** Solo card text when the team is only you: your role for the badge, the champion for the note line. */
function soloBadge(cov: Coverage): { role: string | null; note: string } {
  const mine = cov.assignments.find(a => a.champion && a.player?.isSelf);
  return mine?.champion ? { role: ROLE_LABEL[mine.role], note: `You play ${mine.champion.name}` } : { role: null, note: "You don't play this yet" };
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
