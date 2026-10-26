import { lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { createRoot } from "react-dom/client";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { Popover } from "radix-ui";
import { listen } from "@tauri-apps/api/event";
import { AccountsIcon, DashboardIcon, DraftIcon, LiveGameIcon, MatchupsIcon, PatchNotesIcon, PlayIcon, SettingsIcon, TierlistIcon } from "./ui/icons";
import { BrandIcon } from "../components/BrandIcon";
import { useRiotClientIcon } from "../riotIcon";
import type { ChampionOption } from "../runes/types";
import type { DashboardView, Game } from "./types";
import { Dashboard, DashboardSkeleton, crestSlug, splitRiotId } from "./Dashboard";
import { TitleBar } from "./TitleBar";
import { locationTrail } from './locationTrail';
import { useInGame } from "./LiveGameData";
import { LIVE_PAGE, enterLive, leaveLive, showInNav } from "./LiveGameModel";
import FlyoutApp from "../App";
import { OnboardingGate } from "../onboarding/Onboarding";
import type { AppState, FullAppBridge, View } from "../App";
import "./dashboard.css";
import "./full-app.css";
import { AccountCards } from "./AccountCards";
import { SettingsPage } from "./SettingsPage";
import { ChampionSelect } from "./ChampionSelect";
import { PageHeader } from "./ui/PageHeader";
import { pageLabel } from "./ui/pageNames";
import { AccountPicker, Podium, SwitchOverlay, type PodiumIdentity } from "./ui/Podium";
import { useChampionArt } from "./art";
import { AmbientArt, AmbientArtContext, useAmbientArt } from './AmbientArt';
import { contextualChampion, sidebarChampion, type AmbientChampions } from './lookModel';
import { EmptyState, Notice, Skeleton } from "./ui/States";
import { useRestartClient } from "./restartClient";

const Champions = lazy(() => import("./Champions"));
const PatchNotes = lazy(() => import("./PatchNotes"));
const LiveGame = lazy(() => import("./LiveGame"));
const MatchDetail = lazy(() => import("./MatchDetail"));
const PlayerPage = lazy(() => import('./PlayerPage'));
type MatchLocation = `Match:${number}` | `Match:${number}:${string}`;
type Page = "Dashboard" | "Live Game" | "Tier list & Builds" | "Matchups" | "Patch notes" | MatchLocation | `Player:${string}`;
type AppPage = "Accounts" | "Champion select" | "Settings";
type Location = Page | AppPage;
// Pages hosted by the flyout's own App component, and the App view each opens.
const appViews: Record<AppPage, View> = { Accounts: "accounts", "Champion select": "runes", Settings: "settings" };
const appPageOf = (view: View): AppPage => view === "runes" ? "Champion select" : view === "settings" ? "Settings" : "Accounts";
const groups: { label: string; items: [Location, typeof DashboardIcon][] }[] = [
  { label: "Overview", items: [["Dashboard", DashboardIcon], ["Live Game", LiveGameIcon]] },
  { label: "Champions", items: [["Tier list & Builds", TierlistIcon], ["Matchups", MatchupsIcon], ["Patch notes", PatchNotesIcon], ["Champion select", DraftIcon]] },
  { label: "Swapper", items: [["Accounts", AccountsIcon], ["Settings", SettingsIcon]] },
];
const SEEN_PATCH_KEY = "pn-seen-patch";
// The listing itself is cached for six hours by the backend; an hourly check is mostly a cache hit.
const NOTES_CHECK_MS = 60 * 60 * 1000;
const allPages = groups.flatMap(group => group.items.map(([label]) => label));

function App() {
  const [ambientIds, setAmbientIds] = useState<AmbientChampions>({ champion: null, player: null, draft: null });
  const publishAmbient = useCallback((kind: keyof AmbientChampions, id: number | null) => {
    setAmbientIds(ids => ids[kind] === id ? ids : { ...ids, [kind]: id });
  }, []);
  const visitedMatches = useRef(new Map<string, Game>());
  const [page, setPage] = useState<Page | "app">("Dashboard");
  const [appView, setAppView] = useState<View>("accounts");
  const [request, setRequest] = useState<FullAppBridge["request"]>(null);
  const [inChampSelect, setInChampSelect] = useState(false);
  const [focus, setFocus] = useState<{ champion: ChampionOption | null } | null>(null);
  const [locationChampion, setLocationChampion] = useState<ChampionOption | null>(null);
  const current: Location = page === "app" ? appPageOf(appView) : page;
  const currentRef = useRef(current);
  currentRef.current = current;
  // Where champion select interrupted the user; restored when it ends.
  const returnTo = useRef<Location | null>(null);
  const go = useCallback((target: Location) => {
    if (target in appViews) { setPage("app"); setRequest({ view: appViews[target as AppPage] }); }
    else setPage(target as Page);
  }, []);
  const sidebarGo = (target: Location) => {
    if (target !== currentRef.current) { setFocus(null); go(target); return; }
    const scroller = document.querySelector<HTMLElement>(".dashboard-main")
      ?? document.querySelector<HTMLElement>(".full-app-embed .main-panel");
    scroller?.scrollTo({ top: 0, behavior: matchMedia("(prefers-reduced-motion: reduce)").matches ? "instant" : "smooth" });
  };

  // A running game switches to Live Game; its end restores the earlier page.
  const inGame = useInGame();
  const liveReturn = useRef<Location | null>(null);
  // Where a finished game left the user; its match opens once League files it, unless they moved on.
  const afterGame = useRef<{ from: Location; id: number | null } | null>(null);
  // The end-of-game screen's stats, shown on the dashboard until match history files the game.
  const [ended, setEnded] = useState<Game | null>(null);
  useEffect(() => {
    if (inGame) {
      const next = enterLive(currentRef.current, liveReturn.current, returnTo.current);
      liveReturn.current = next.returnTo as Location;
      returnTo.current = null;
      go(next.page as Location);
    } else if (liveReturn.current) {
      const back = leaveLive(currentRef.current, liveReturn.current);
      liveReturn.current = null;
      afterGame.current = back ? { from: back as Location, id: null } : null;
      if (back) go(back as Location);
    }
  }, [inGame, go]);

  // Back/forward over visited pages, browser style.
  const [history, setHistory] = useState<{ stack: Location[]; index: number }>({ stack: ["Dashboard"], index: 0 });
  const travelling = useRef(false);
  useEffect(() => {
    if (travelling.current) { travelling.current = false; return; }
    setHistory(h => h.stack[h.index] === current ? h : { stack: [...h.stack.slice(0, h.index + 1), current].slice(-50), index: Math.min(h.index + 1, 49) });
  }, [current]);
  const travel = (step: number) => {
    const index = history.index + step;
    if (index < 0 || index >= history.stack.length) return;
    travelling.current = true;
    setHistory({ ...history, index });
    go(history.stack[index]);
  };

  const [view, setView] = useState<DashboardView | null>(null);
  const signedIn = view && !view.stale ? view.snapshot?.summoner.riotId ?? null : null;
  const restartFn = useRef(() => {});
  const bridge = useMemo<FullAppBridge>(() => ({
    request,
    signedIn,
    onRestart: () => restartFn.current(),
    Accounts: AccountCards,
    Settings: SettingsPage,
    Runes: ChampionSelect,
    onViewChange: setAppView,
    onChampSelectChange: active => {
      setInChampSelect(active);
      if (active) {
        // App switches itself to its champion select view.
        returnTo.current = currentRef.current;
        setPage("app");
        return;
      }
      const back = returnTo.current;
      returnTo.current = null;
      if (back && currentRef.current === "Champion select") go(back === "Champion select" ? "Accounts" : back);
    },
  }), [request, go, signedIn]);

  const [region, setRegion] = useState<string | null>(null);
  const [accounts, setAccounts] = useState<AppState | null>(null);
  const [phase, setPhase] = useState("");
  const [champions, setChampions] = useState<ChampionOption[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [pickerOpen, setPickerOpen] = useState(false);
  const [patch, setPatch] = useState<string | null>(null);
  // Newest patch in Riot's notes listing (not the game patch: notes can land before or after it).
  // A version newer than the last one opened earns the sidebar's NEW badge.
  const [notesPatch, setNotesPatch] = useState<string | null>(null);
  const [seenPatch, setSeenPatch] = useState(() => { try { return localStorage.getItem(SEEN_PATCH_KEY); } catch { return null; } });
  useEffect(() => {
    if (!isTauri()) return;
    let live = true;
    const check = () => void invoke<string>("newest_patch_notes").then(p => { if (live) setNotesPatch(p); }).catch(() => {});
    check();
    const timer = window.setInterval(check, NOTES_CHECK_MS);
    return () => { live = false; window.clearInterval(timer); };
  }, []);
  useEffect(() => {
    if (current !== "Patch notes" || !notesPatch || seenPatch === notesPatch) return;
    setSeenPatch(notesPatch);
    try { localStorage.setItem(SEEN_PATCH_KEY, notesPatch); } catch { /* badge only */ }
  }, [current, notesPatch, seenPatch]);
  // The switch flow reports no stages, only started / done / failed.
  const [switching, setSwitching] = useState<{ id: string; name: string; restart: boolean } | null>(null);
  const activeIdRef = useRef<string | null>(null);
  // Backend watch: League Client is closed, or runs but its API or window stopped answering.
  const [clientHealth, setClientHealth] = useState("ok");
  const riotIcon = useRiotClientIcon();
  const clientClosed = useRef(false);
  clientClosed.current = clientHealth === "closed";
  const fetching = useRef(false);
  const mounted = useRef(false);
  const queued = useRef(false);
  const lastPhase = useRef("");
  async function refresh() {
    if (!isTauri()) { setError("Open Swapper from the tray to connect to League Client."); return; }
    if (fetching.current) { queued.current = true; return; }
    fetching.current = true; setLoading(true);
    void invoke<AppState>("get_state").then(state => {
      if (mounted.current) { setAccounts(state); setRegion(state.accounts.find(a => a.id === state.activeId)?.region ?? null); }
    }).catch(() => {});
    try { const next = await invoke<DashboardView>("dashboard_snapshot"); if (mounted.current) { setView(next); setError(""); } }
    catch (reason) { if (mounted.current) setError(String(reason)); }
    finally {
      fetching.current = false;
      if (mounted.current) { setLoading(false); if (queued.current) { queued.current = false; void refresh(); } }
    }
  }
  useEffect(() => {
    mounted.current = true;
    const focusRefresh = () => { if (document.visibilityState === "visible") void refresh(); };
    window.addEventListener("focus", focusRefresh);
    const stop = isTauri() ? listen<{ phase: string }>("champ_select", ({ payload }) => {
      if (lastPhase.current !== payload.phase) { lastPhase.current = payload.phase; setPhase(payload.phase); void refresh(); }
    }) : Promise.resolve(() => {});
    // Tray menu destinations (Add Account, Settings, ...) are App views.
    const navigate = isTauri() ? listen<string>("navigate", ({ payload }) => setPage(payload === "live" ? LIVE_PAGE : "app")) : Promise.resolve(() => {});
    const switches = isTauri() ? Promise.all([
      // Switching onto the signed-in account is a client restart.
      listen<{ id: string; name: string }>("switch_started", ({ payload }) => { setPickerOpen(false); setSwitching({ ...payload, restart: payload.id === activeIdRef.current && !clientClosed.current }); }),
      listen("switch_done", () => { setSwitching(null); void refresh(); }),
      listen("switch_failed", () => setSwitching(null)),
    ]) : Promise.resolve([]);
    const health = isTauri() ? listen<string>("client_health", ({ payload }) => setClientHealth(payload)) : Promise.resolve(() => {});
    // A saved account's icon was refreshed: the picker shows it.
    const accountsChanged = isTauri() ? listen("accounts_changed", () => { void invoke<AppState>("get_state").then(state => { if (mounted.current) setAccounts(state); }).catch(() => {}); }) : Promise.resolve(() => {});
    if (isTauri()) void invoke<string>("client_health").then(state => { if (mounted.current) setClientHealth(state); }).catch(() => {});
    if (isTauri()) void invoke<ChampionOption[]>("get_champion_list").then(list => { if (mounted.current) setChampions(list); }).catch(() => {});
    if (isTauri()) void invoke<string | null>("get_current_patch").then(p => { if (mounted.current) setPatch(p); }).catch(() => {});
    void refresh();
    return () => {
      mounted.current = false;
      window.removeEventListener("focus", focusRefresh);
      void navigate.then(unlisten => unlisten());
      void switches.then(stops => stops.forEach(unlisten => unlisten()));
      void health.then(unlisten => unlisten());
      void accountsChanged.then(unlisten => unlisten());
      void stop.then(unlisten => unlisten());
    };
  }, []);
  const openChampion = (champion: ChampionOption) => { setFocus({ champion }); go("Tier list & Builds"); };
  const championNames = useMemo(() => new Map(champions.map(c => [c.id, c.name])), [champions]);
  const snapshot = view?.snapshot;
  const rank = snapshot?.rank;
  const matchParts = page.startsWith('Match:') ? page.split(':') : null;
  const matchId = matchParts ? Number(matchParts[1]) : null;
  const matchPlayer = matchParts?.[2] ? decodeURIComponent(matchParts[2]) : undefined;
  const playerId = page.startsWith('Player:') ? page.slice(7) : null;
  useEffect(() => {
    if (!isTauri()) return;
    // game_ended: the just-ended game from the end-of-game screen, before match history has it.
    const shown = listen<Game>("game_ended", ({ payload }) => {
      if (afterGame.current) afterGame.current = { ...afterGame.current, id: payload.id };
      setEnded(payload);
    });
    const stop = listen<number>("game_finished", ({ payload }) => {
      if (afterGame.current) afterGame.current = { ...afterGame.current, id: payload };
      void refresh();
    });
    return () => { void stop.then(unlisten => unlisten()); void shown.then(unlisten => unlisten()); };
  }, []);
  useEffect(() => {
    if (ended && snapshot?.games.some(game => game.id === ended.id)) setEnded(null);
  }, [ended, snapshot]);
  useEffect(() => {
    const pending = afterGame.current;
    if (!pending?.id || (ended?.id !== pending.id && !snapshot?.games.some(game => game.id === pending.id))) return;
    afterGame.current = null;
    if (currentRef.current === pending.from) go(`Match:${pending.id}`);
  }, [snapshot, ended, go]);
  const selectedMatch = matchId == null ? null : visitedMatches.current.get(page) ?? snapshot?.games.find(game => game.id === matchId) ?? (ended?.id === matchId ? ended : undefined);
  const activeAccount = accounts?.accounts.find(account => account.id === accounts.activeId);
  const [gameName, tag] = splitRiotId(snapshot?.summoner.riotId ?? activeAccount?.riotId ?? "");
  const who: PodiumIdentity = {
    name: gameName || activeAccount?.name || "League Client", tag: tag.slice(1) || null,
    icon: snapshot?.summoner.icon ?? activeAccount?.iconDataUrl ?? null, level: snapshot?.summoner.level ?? null,
    region, rank: rank ?? null, unranked: Boolean(snapshot),
  };
  // Connected = the client reported who is signed in (not saved stats, not an error).
  const connected = Boolean(signedIn) && !error;
  // Keep the sidebar's profile art distinct from the contextual canvas when another champion is available.
  const mainChampion = contextualChampion(current, ambientIds, selectedMatch?.championId ?? null);
  const pageArt = useAmbientArt(mainChampion);
  const profileChampions = snapshot?.summaries.all?.champions;
  const ambient = useChampionArt(sidebarChampion(profileChampions?.[0]?.id, profileChampions?.[1]?.id, mainChampion), "splash");
  activeIdRef.current = accounts?.activeId ?? null;
  const switchingAccount = switching ? accounts?.accounts.find(account => account.id === switching.id) : undefined;
  const switchTo = (id: string) => { setPickerOpen(false); setPage("app"); setRequest({ view: "accounts", switchId: id }); };
  // Every restart entry point (picker, Accounts card, Ctrl+K, status offer) comes through here.
  const { request: restartClient, dialog: restartDialog } = useRestartClient(() => {
    const id = activeIdRef.current;
    if (id) switchTo(id);
    return Boolean(id);
  });
  restartFn.current = () => void restartClient();
  return <AmbientArtContext.Provider value={publishAmbient}><div className="fa-frame" data-look="glass-context" style={ambient ? { "--ambient-art": `url("${ambient}")` } as CSSProperties : undefined}>
    <TitleBar canBack={history.index > 0} canForward={history.index < history.stack.length - 1} onBack={() => travel(-1)} onForward={() => travel(1)}
      trail={locationTrail(current, { champion: locationChampion?.name, matchPlayer, match: selectedMatch ? {
        champion: snapshot?.championNames[selectedMatch.championId] ?? championNames.get(selectedMatch.championId) ?? 'Match', win: selectedMatch.win,
      } : null })} onPickLocation={target => { setFocus({ champion: null }); go(target as Location); }}
      onRefresh={() => void refresh()} refreshing={loading} champions={champions} pages={allPages.filter(p => showInNav(p, inGame))}
      onPickChampion={openChampion} onPickPage={page => go(page as Location)} snapshot={snapshot}
      onPickPlayer={riotId => go(`Player:${riotId}`)} onRestart={() => void restartClient()} closed={clientHealth === "closed"} />
    <div className="dashboard-shell">
      <AmbientArt src={pageArt} />
      <aside className="sidebar dash-scope">
        <Popover.Root open={pickerOpen} onOpenChange={setPickerOpen}>
          <div className="podium-wrap"><Podium who={who} connected={connected} open={pickerOpen} crest={rank ? crestSlug(rank.tier) : null} disabled={Boolean(switching)}
            trouble={(clientHealth === "dead" || clientHealth === "hung") && !switching && !inGame} onRestart={() => void restartClient()} onOpenProfile={who.tag ? () => go(`Player:${who.name}#${who.tag}`) : undefined} />
            {pickerOpen && <AccountPicker accounts={accounts?.accounts ?? []} activeId={accounts?.activeId ?? null} activeRank={rank ?? null} cards={view?.cards ?? {}} disabled={Boolean(switching)} closed={clientHealth === "closed"}
              onSwitch={switchTo} onRestart={() => { setPickerOpen(false); void restartClient(); }} onManage={() => { setPickerOpen(false); go("Accounts"); }} />}
          </div>
        </Popover.Root>
        <nav aria-label="Swapper pages">
          {groups.map(group => <div className="nav-group" key={group.label} role="group" aria-label={group.label}>
            {group.items.filter(([label]) => showInNav(label, inGame)).map(([label, Icon]) => <button key={label} aria-current={current === label ? "page" : undefined} onClick={() => sidebarGo(label)}>
              <Icon size={19} aria-hidden />{pageLabel(label)}
              {label === "Tier list & Builds" && patch && <span className="nav-badge patch" data-label={`PATCH ${patch}`} title={`Patch ${patch}`} />}
              {label === "Patch notes" && notesPatch && seenPatch !== notesPatch && current !== label && <span className="nav-badge new" title={`Patch ${notesPatch} notes are out`}>NEW!</span>}
              {label === "Champion select" && inChampSelect && <span className="nav-badge live"><i aria-hidden />LIVE</span>}
            </button>)}
          </div>)}
        </nav>
        {/* The restart flow opens the client when none runs. */}
        {clientHealth === "closed" && !switching && !inGame && <button type="button" className="sidebar-play" onClick={() => void restartClient()}>
          {riotIcon ? <img src={riotIcon} alt="" /> : <PlayIcon size={22} aria-hidden />}<span>Play</span>
        </button>}
      </aside>
      {restartDialog}
      {switching && <SwitchOverlay account={switchingAccount} name={switching.name} restart={switching.restart} />}
      <OnboardingGate />
      <div className="full-app-embed" style={page === "app" ? undefined : { display: "none" }}><FlyoutApp fullApp={bridge} /></div>
      {page !== "app" && <main className="dashboard-main dash-scope">
        {selectedMatch ? <PageHeader
            title={`${snapshot?.championNames[selectedMatch.championId] ?? championNames.get(selectedMatch.championId) ?? "Match"} · ${selectedMatch.win ? "Victory" : "Defeat"}`} />
          : <PageHeader title={playerId != null ? 'Player profile' : matchId != null ? "Match detail" : pageLabel(page)}
            actions={page === "Dashboard" && snapshot && <small className="updated">Updated {fmtAgoShort(snapshot.fetchedAt)}</small>} />}
        {error && <Notice tone="error">{error}</Notice>}
        {page === "Dashboard"
          ? snapshot ? <Dashboard snapshot={snapshot} phase={phase} justEnded={ended} onOpenLive={() => go(LIVE_PAGE)}
              onOpenChampion={(id, name) => openChampion({ id, name })}
              onOpenMatch={game => { const target = `Match:${game.id}` as const; visitedMatches.current.set(target, game); go(target); }} />
            : loading ? <DashboardSkeleton />
            : <EmptyState icon={<BrandIcon size={40} />} title="Sign in to League Client">Open League Client to see your games and rank.</EmptyState>
          : playerId != null ? <Suspense fallback={<Skeleton label="Loading player profile" rows={4} height={64} />}><PlayerPage key={playerId} riotId={playerId} snapshot={snapshot}
              names={snapshot?.championNames ?? Object.fromEntries(championNames)} onBack={() => travel(-1)} onOpenPlayer={riotId => go(`Player:${riotId}`)}
              onOpenMatch={game => { const target = `Match:${game.id}:${encodeURIComponent(playerId)}` as const; visitedMatches.current.set(target, game); go(target); }} /></Suspense>
          : selectedMatch ? <Suspense fallback={<Skeleton label="Loading match details" rows={4} height={64} />}><MatchDetail key={page} game={selectedMatch} riotId={matchPlayer} names={snapshot?.championNames ?? Object.fromEntries(championNames)} /></Suspense>
          : page === "Live Game" ? <Suspense fallback={<Skeleton label="Loading Live Game" rows={5} height={56} />}><LiveGame championNames={championNames} /></Suspense>
          : page === "Patch notes" ? <Suspense fallback={<Skeleton label="Loading patch notes" rows={6} height={72} />}><PatchNotes /></Suspense>
          : <Suspense fallback={<Skeleton label="Loading champion tools" rows={6} height={52} />}>
              <Champions key={page} matchups={page === "Matchups"} focus={page === 'Matchups' && focus?.champion ? null : focus} onSelectionChange={setLocationChampion} />
            </Suspense>}
      </main>}
    </div>
  </div></AmbientArtContext.Provider>;
}

function fmtAgoShort(at: number) {
  const minutes = Math.floor((Date.now() - at) / 60000);
  return minutes < 1 ? "just now" : minutes < 60 ? `${minutes}m ago` : `${Math.floor(minutes / 60)}h ago`;
}

createRoot(document.getElementById("root")!).render(<App />);
