import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { LockKeyhole, Search, X } from "lucide-react";
import { fmtPct, type ChampionCountersView, type TierListView } from "../runes/types";
import type { RecommendedPick } from "./types";
import { ChampionArt } from "./MatchupStrip";
import { NearViewport } from "./NearViewport";
import { counterPicks, defaultTab, metaPicks, recommendedSuggestions, searchChampions, type Suggestion, type SuggestionTab } from "./champ-select-model";

type Loaded = { key: string; picks: Suggestion[] | null; message: string };
const TABS: [SuggestionTab, string][] = [["counters", "Counters"], ["recommended", "Recommended"], ["meta", "Meta"]];

/** Planning mode: pick suggestions. A click hovers the champion in the client;
 *  locking in takes a second, confirming click. */
export function PickSuggestions({ role, tier, enemyId, enemyName, taken, names, hovered, tierList }: {
  role: string; tier: string; enemyId: number; enemyName: string; taken: Set<number>; names: Map<number, string>;
  hovered: number; tierList: (position: string, tier: string) => Promise<TierListView>;
}) {
  // The default follows the enemy laner until the user picks a tab.
  const [chosenTab, setChosenTab] = useState<SuggestionTab | null>(null);
  const tab = chosenTab ?? defaultTab(enemyId);
  const key = `${tab}:${role}:${tier}:${tab === "counters" ? enemyId : 0}`;
  const cache = useRef(new Map<string, Loaded>());
  const [loaded, setLoaded] = useState<Loaded | null>(null);
  const [pending, setPending] = useState(0);
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
    if (!role || role === "none") done(null, "Choose your role to see suggestions.");
    else if (tab === "counters" && !enemyId) done(null, "Counters appear once your lane opponent is revealed.");
    else if (tab === "counters") void invoke<ChampionCountersView>("full_app_matchups", { championId: enemyId, position: role, tier })
      .then(view => done(counterPicks(view.beats, new Set(), 24))).catch(reason => done(null, String(reason)));
    else if (tab === "recommended") void invoke<RecommendedPick[]>("recommended_picks", { role, tier })
      .then(picks => done(recommendedSuggestions(picks, names, new Set()))).catch(reason => done(null, String(reason)));
    else void tierList(role, tier).then(view => done(metaPicks(view.rows, new Set(), 24))).catch(reason => done(null, String(reason)));
    return () => { live = false; };
  }, [key]);

  // Cached lists keep every champion; bans and locks are filtered per render.
  const picks = (loaded?.key === key ? loaded.picks ?? [] : []).filter(pick => !taken.has(pick.championId)).slice(0, 8)
    .map(pick => ({ ...pick, name: names.get(pick.championId) ?? pick.name }));
  // Typing searches every champion; win rates show when the current tab has them.
  const known = new Map((loaded?.key === key ? loaded.picks ?? [] : []).map(pick => [pick.championId, pick]));
  const found = searchChampions(names, query, taken).map(c => ({ ...known.get(c.championId), ...c }) as Card);
  const hover = async (id: number) => {
    setPending(id); setNotice("");
    try { await invoke("full_app_hover_champion", { championId: id }); setQuery(""); }
    catch (reason) { setNotice(String(reason)); }
    finally { setPending(0); }
  };
  const subtitle = tab === "counters" ? `Your win rate into ${enemyName || "the enemy laner"}` : tab === "recommended" ? "Safe blind picks for your role and rank" : "Strongest picks this patch for your role";

  return <section className="fa-card cs-picks">
    <div className="fa-section-heading">
      <div><h3>Pick suggestions</h3><p>{subtitle}</p></div>
      <div className="cs-picks-tools">
        <label className="cs-search"><Search size={15} aria-hidden /><input value={query} onChange={event => setQuery(event.target.value)} placeholder="Search champion" aria-label="Search champion" />
          {query && <button aria-label="Clear search" onClick={() => setQuery("")}><X size={14} /></button>}</label>
        <div className="fa-seg" role="group" aria-label="Suggestions">{TABS.map(([value, label]) => <button key={value} aria-pressed={tab === value && !query} onClick={() => { setChosenTab(value); setQuery(""); }}>{label}</button>)}</div>
      </div>
    </div>
    {notice && <p className="fa-notice" role="alert">{notice}</p>}
    {query.trim() ? !found.length ? <p className="fa-panel-empty">No champion matches "{query.trim()}".</p>
      : <div className="cs-pick-grid">{found.map(pick => <PickCard key={pick.championId} pick={pick} hovered={hovered} pending={pending} onHover={hover} />)}</div>
      : !loaded || loaded.key !== key ? <p className="fa-panel-empty" role="status">Loading suggestions…</p>
      : !picks.length ? <p className="fa-panel-empty">{loaded.message || "No suggestions for this role and rank."}</p>
      : <div className="cs-pick-grid">{picks.map(pick => <PickCard key={pick.championId} pick={pick} hovered={hovered} pending={pending} onHover={hover} />)}</div>}
  </section>;
}

type Card = Partial<Suggestion> & { championId: number; name: string };

function PickCard({ pick, hovered, pending, onHover }: { pick: Card; hovered: number; pending: number; onHover: (id: number) => void }) {
  const stats = pick.winPct != null && pick.games != null;
  return <button className="cs-pick" aria-pressed={pick.championId === hovered} disabled={!!pending} onClick={() => onHover(pick.championId)}
    title={stats ? `${pick.name}: ${fmtPct(pick.winPct!)} over ${pick.games!.toLocaleString()} games` : pick.name}>
    <NearViewport className="cs-pick-art"><ChampionArt id={pick.championId} name={pick.name} kind="loading" /></NearViewport>
    <span className="cs-pick-copy"><strong>{pick.name}</strong>{stats && <><b>{fmtPct(pick.winPct!)} <small>WR</small></b><small>{pick.games!.toLocaleString()} games</small></>}{pick.flex && <span className="cs-flex">Flex</span>}</span>
    {pick.championId === pending && <span className="cs-pick-state">Hovering…</span>}
    {pick.championId === hovered && pending !== pick.championId && <span className="cs-pick-state">Hovered</span>}
  </button>;
}

/** A small Lock in button that always asks once more; a plain click never locks. */
export function LockIn({ championId, name }: { championId: number; name: string }) {
  const [confirming, setConfirming] = useState(false);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState("");
  useEffect(() => { setConfirming(false); setNotice(""); }, [championId]);
  const lock = async () => {
    setBusy(true); setNotice("");
    try { await invoke("full_app_lock_in", { championId }); }
    catch (reason) { setNotice(String(reason)); }
    finally { setBusy(false); setConfirming(false); }
  };
  return <div className="cs-lock">
    {confirming ? <><span>Lock in <strong>{name}</strong>? You cannot change it afterwards.</span><div><button className="fa-secondary" onClick={() => setConfirming(false)}>Cancel</button><button className="fa-primary" disabled={busy} onClick={() => void lock()}><LockKeyhole size={14} />{busy ? "Locking…" : "Confirm"}</button></div></>
      : <><span>{championId ? <>Ready to play <strong>{name}</strong>?</> : "Choose a champion to lock in."}</span><div><button className="fa-primary" disabled={!championId} onClick={() => setConfirming(true)}><LockKeyhole size={14} />Lock in</button></div></>}
    {notice && <p className="fa-notice" role="alert">{notice}</p>}
  </div>;
}
