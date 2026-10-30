import { useEffect, useRef, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { LockKeyhole, X, Ban } from "lucide-react";
import { SearchIcon } from "./ui/icons";
import { fmtPct, type ChampionCountersView, type TierListView } from "../runes/types";
import type { RecommendedPick } from "./types";
import { ChampionArt } from "./MatchupStrip";
import { NearViewport } from "./NearViewport";
import { Notice, Skeleton } from "./ui/States";
import { fmtCount } from "./ui/format";
import { allChampions, counterPicks, defaultTab, recommendedSuggestions, searchChampions, type Suggestion, type SuggestionTab } from "./champ-select-model";

type Loaded = { key: string; picks: Suggestion[] | null; message: string };
const TABS: [SuggestionTab, string][] = [["all", "All"], ["recommended", "Recommended"], ["counters", "Counters"]];

/** A card click declares intent; its separate Lock in button selects and locks. */
export function PickSuggestions({ role, tier, enemyIds, taken, names, hovered: pickHover, tierList, controls, canLock, banTurn, pickable }: {
  /** Your likely lane opponents, most likely first; several when ranked hides enemy roles. */
  role: string; tier: string; enemyIds: number[]; taken: Set<number>; names: Map<number, string>;
  hovered: number; tierList: (position: string, tier: string) => Promise<TierListView>;
  controls: ReactNode; canLock: boolean; banTurn?: number; pickable: Set<number>;
}) {
  // The default follows the enemy laner until the user picks a tab.
  const [chosenTab, setChosenTab] = useState<SuggestionTab | null>(null);
  const [chosenEnemy, setChosenEnemy] = useState(0);
  const enemyId = enemyIds.includes(chosenEnemy) ? chosenEnemy : enemyIds[0] ?? 0;
  const enemyName = names.get(enemyId) ?? "";
  const tab = chosenTab ?? defaultTab(enemyIds.length === 1 ? enemyId : 0);
  const key = `${tab}:${role}:${tier}:${tab === "counters" ? enemyId : 0}`;
  const cache = useRef(new Map<string, Loaded>());
  const [loaded, setLoaded] = useState<Loaded | null>(null);
  const [pending, setPending] = useState(0);
  const [locking, setLocking] = useState(0);
  const acting = useRef(false);
  const [notice, setNotice] = useState("");
  const [query, setQuery] = useState("");

  useEffect(() => {
    const cached = cache.current.get(key);
    if (cached) { setLoaded(cached); return; }
    let live = true;
    const done = (picks: Suggestion[] | null, message = "") => {
      const value = { key, picks, message };
      if (picks) cache.current.set(key, value);
      if (live) setLoaded(value);
    };
    setLoaded(null);
    if (!role || role === "none") done(null, "Choose your role.");
    else if (tab === "counters" && !enemyId) done(null, "Waiting for your lane opponent.");
    else if (tab === "counters") void invoke<ChampionCountersView>("full_app_matchups", { championId: enemyId, position: role, tier })
      .then(view => done(counterPicks(view.beats, new Set(), 24))).catch(reason => done(null, String(reason)));
    else if (tab === "recommended") void invoke<RecommendedPick[]>("recommended_picks", { role, tier })
      .then(picks => done(recommendedSuggestions(picks, names, new Set()))).catch(reason => done(null, String(reason)));
    else void tierList(role, tier).then(view => done(allChampions(names, view.rows, new Set()))).catch(() => done(allChampions(names, [], new Set())));
    return () => { live = false; };
  }, [key]);

  // Cached lists keep every champion; bans, locks and champions you don't own are filtered per render.
  // An empty pickable set means the client hasn't said yet, so nothing is hidden.
  // On your ban turn any champion can be banned, and the selection is your ban hover.
  const banning = banTurn !== undefined;
  const hovered = banning ? banTurn : pickHover;
  const ownedOrFree = (id: number) => banning || !pickable.size || pickable.has(id);
  const picks = (loaded?.key === key ? loaded.picks ?? [] : []).filter(pick => !taken.has(pick.championId) && ownedOrFree(pick.championId)).slice(0, tab === "all" ? undefined : 8)
    .map(pick => ({ ...pick, name: names.get(pick.championId) ?? pick.name }));
  // Typing searches every champion; win rates show when the current tab has them.
  const known = new Map((loaded?.key === key ? loaded.picks ?? [] : []).map(pick => [pick.championId, pick]));
  const found = searchChampions(names, query, taken).filter(c => ownedOrFree(c.championId)).map(c => ({ ...known.get(c.championId), ...c }) as Card);
  const hover = async (id: number) => {
    if (acting.current || taken.has(id)) return;
    acting.current = true;
    setPending(id); setNotice("");
    // The search stays: clearing it would hide the chosen card and its Lock in button.
    try { await invoke(banning ? "full_app_hover_ban" : "full_app_hover_champion", { championId: id }); }
    catch (reason) { setNotice(String(reason)); }
    finally { setPending(0); acting.current = false; }
  };
  const lock = async (id: number) => {
    if (acting.current || !(banning || (canLock && pickable.has(id))) || id <= 0 || taken.has(id)) return;
    acting.current = true;
    setLocking(id); setNotice("");
    try {
      // Select this card first; the lock or ban command rechecks the live turn and champion.
      await invoke(banning ? "full_app_hover_ban" : "full_app_hover_champion", { championId: id });
      await invoke(banning ? "full_app_ban" : "full_app_lock_in", { championId: id });
    } catch (reason) { setNotice(String(reason)); }
    finally { setLocking(0); acting.current = false; }
  };
  const lockable = hovered > 0 && !taken.has(hovered) && (banning || (canLock && pickable.has(hovered)));
  const action = banning ? "Ban" : "Lock in";
  const subtitle = tab === "counters" ? `Your win rate into ${enemyName || "the enemy laner"}` : tab === "recommended" ? "Safe blind picks for your role" : "Every champion in this role, A to Z";

  return <section className="fa-card cs-picks">
    {/* Role and rank left, one Lock in centre (for the selected champion, wherever its card moves), search and tabs right. */}
    <div className="fa-section-heading cs-picks-head">
      <div className="cs-picks-tools is-left">{controls}</div>
      <button type="button" className={`cs-lock-in${banning ? " is-ban" : ""}`} disabled={!lockable || !!pending || !!locking} aria-busy={!!locking}
        title={lockable ? `${action} ${names.get(hovered) ?? "this champion"}` : banning ? "Select a champion to ban" : canLock ? "Select a champion to lock in" : "Wait for your pick turn"}
        onClick={() => void lock(hovered)}>
        {banning ? <Ban size={16} aria-hidden /> : <LockKeyhole size={16} aria-hidden />}<span>{locking ? (banning ? "Banning…" : "Locking…") : action}</span>
      </button>
      <div className="cs-picks-tools" title={subtitle}>
        <label className="cs-search"><SearchIcon size={15} aria-hidden /><input value={query} onChange={event => setQuery(event.target.value)} placeholder="Search champion" aria-label="Search champion" />
          {query && <button aria-label="Clear search" onClick={() => setQuery("")}><X size={14} /></button>}</label>
        <div className="fa-seg" role="group" aria-label="Suggestions">{TABS.map(([value, label]) => <button key={value} aria-pressed={tab === value && !query} onClick={() => { setChosenTab(value); setQuery(""); }}>{label}</button>)}</div>
      </div>
    </div>
    {notice && <Notice tone="error">{notice}</Notice>}
    {tab === "counters" && !query.trim() && enemyIds.length > 1 && <div className="fa-seg cs-laner-pick" role="group" aria-label="Possible lane opponent">
      <small>Counters for</small>{enemyIds.map(id => <button key={id} aria-pressed={id === enemyId} onClick={() => setChosenEnemy(id)}>{names.get(id) ?? "Unknown"}</button>)}</div>}
    {query.trim() ? !found.length ? <p className="fa-panel-empty">No champion matches "{query.trim()}".</p>
      : <div className="cs-pick-grid" role="group" aria-label="Champion suggestions" tabIndex={0}>{found.map(pick => <PickCard key={pick.championId} pick={pick} hovered={hovered} pending={pending} locking={locking} onHover={hover} />)}</div>
      : !loaded || loaded.key !== key ? <Skeleton label="Loading suggestions" rows={3} height={64} />
      : !picks.length ? <p className="fa-panel-empty">{loaded.message || "No suggestions for this role and rank."}</p>
      : <div className="cs-pick-grid" role="group" aria-label="Champion suggestions" tabIndex={0}>{picks.map(pick => <PickCard key={pick.championId} pick={pick} hovered={hovered} pending={pending} locking={locking} onHover={hover} />)}</div>}
  </section>;
}

type Card = Partial<Suggestion> & { championId: number; name: string };

function PickCard({ pick, hovered, pending, locking, onHover }: { pick: Card; hovered: number; pending: number; locking: number; onHover: (id: number) => void }) {
  const stats = pick.winPct != null && !Number.isNaN(pick.winPct) && !!pick.games;
  return <article className={`cs-pick${pick.championId === hovered ? " is-hovered" : ""}${pick.championId === locking ? " is-locking" : ""}`}>
    <button type="button" className="cs-pick-select" aria-label={`Select ${pick.name}`} aria-pressed={pick.championId === hovered} disabled={!!pending || !!locking} onClick={() => onHover(pick.championId)}
    title={stats ? `${pick.name}: ${fmtPct(pick.winPct!)} over ${fmtCount(pick.games!)} games` : pick.name}>
    <NearViewport className="cs-pick-art"><ChampionArt id={pick.championId} name={pick.name} kind="loading" /></NearViewport>
    <span className="cs-pick-copy"><strong>{pick.name}</strong>{stats && <><b>{fmtPct(pick.winPct!)} <small>WR</small></b><small>{fmtCount(pick.games!)} games</small></>}{pick.flex && <span className="cs-flex">Flex</span>}</span>
    {pick.championId === pending && <span className="cs-pick-state">Hovering…</span>}
    {pick.championId === hovered && pending !== pick.championId && <span className="cs-pick-state">Hovered</span>}
    </button>
  </article>;
}
