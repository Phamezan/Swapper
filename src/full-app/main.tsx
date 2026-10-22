import { lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { createRoot } from "react-dom/client";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { AccountsIcon, DashboardIcon, DraftIcon, LiveGameIcon, MatchupsIcon, SettingsIcon, TierlistIcon } from "./ui/icons";
import { BrandIcon } from "../components/BrandIcon";
import type { ChampionOption } from "../runes/types";
import type { DashboardView, Game } from "./types";
import { Dashboard, DashboardSkeleton, crestSlug, splitRiotId } from "./Dashboard";
import { TitleBar } from "./TitleBar";
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
import { EmptyState, Notice, Skeleton } from "./ui/States";

const Champions = lazy(() => import("./Champions"));
const LiveGame = lazy(() => import("./LiveGame"));
const MatchDetail = lazy(() => import("./MatchDetail"));
const PlayerPage = lazy(() => import('./PlayerPage'));
type MatchLocation = `Match:${number}` | `Match:${number}:${string}`;
type Page = "Dashboard" | "Live Game" | "Tier list & Builds" | "Matchups" | MatchLocation | `Player:${string}`;
type AppPage = "Accounts" | "Champion select" | "Settings";
type Location = Page | AppPage;
// Pages hosted by the flyout's own App component, and the App view each opens.
const appViews: Record<AppPage, View> = { Accounts: "accounts", "Champion select": "runes", Settings: "settings" };
const appPageOf = (view: View): AppPage => view === "runes" ? "Champion select" : view === "settings" ? "Settings" : "Accounts";
const groups: { label: string; items: [Location, typeof DashboardIcon][] }[] = [
  { label: "Overview", items: [["Dashboard", DashboardIcon], ["Live Game", LiveGameIcon]] },
  { label: "Champions", items: [["Tier list & Builds", TierlistIcon], ["Matchups", MatchupsIcon], ["Champion select", DraftIcon]] },
  { label: "Swapper", items: [["Accounts", AccountsIcon], ["Settings", SettingsIcon]] },
];
const allPages = groups.flatMap(group => group.items.map(([label]) => label));

function App() {
  const visitedMatches = useRef(new Map<string, Game>());
  const [page, setPage] = useState<Page | "app">("Dashboard");
  const [appView, setAppView] = useState<View>("accounts");
  const [request, setRequest] = useState<FullAppBridge["request"]>(null);
  const [inChampSelect, setInChampSelect] = useState(false);
  const [focus, setFocus] = useState<{ champion: ChampionOption } | null>(null);
  const current: Location = page === "app" ? appPageOf(appView) : page;
  const currentRef = useRef(current);
  currentRef.current = current;
  // Where champion select interrupted the user; restored when it ends.
  const returnTo = useRef<Location | null>(null);
  const go = useCallback((target: Location) => {
    if (target in appViews) { setPage("app"); setRequest({ view: appViews[target as AppPage] }); }
    else setPage(target as Page);
  }, []);

  // A running game switches to Live Game; its end restores the earlier page.
  const inGame = useInGame();
  const liveReturn = useRef<Location | null>(null);
  useEffect(() => {
    if (inGame) {
      const next = enterLive(currentRef.current, liveReturn.current, returnTo.current);
      liveReturn.current = next.returnTo as Location;
      returnTo.current = null;
      go(next.page as Location);
    } else if (liveReturn.current) {
      const back = leaveLive(currentRef.current, liveReturn.current);
      liveReturn.current = null;
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
  const bridge = useMemo<FullAppBridge>(() => ({
    request,
    signedIn,
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
  // The switch flow reports no stages, only started / done / failed.
  const [switching, setSwitching] = useState<{ id: string; name: string } | null>(null);
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
      listen<{ id: string; name: string }>("switch_started", ({ payload }) => { setPickerOpen(false); setSwitching(payload); }),
      listen("switch_done", () => { setSwitching(null); void refresh(); }),
      listen("switch_failed", () => setSwitching(null)),
    ]) : Promise.resolve([]);
    if (isTauri()) void invoke<ChampionOption[]>("get_champion_list").then(list => { if (mounted.current) setChampions(list); }).catch(() => {});
    void refresh();
    return () => {
      mounted.current = false;
      window.removeEventListener("focus", focusRefresh);
      void navigate.then(unlisten => unlisten());
      void switches.then(stops => stops.forEach(unlisten => unlisten()));
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
  const selectedMatch = matchId == null ? null : visitedMatches.current.get(page) ?? snapshot?.games.find(game => game.id === matchId);
  const activeAccount = accounts?.accounts.find(account => account.id === accounts.activeId);
  const [gameName, tag] = splitRiotId(snapshot?.summoner.riotId ?? activeAccount?.riotId ?? "");
  const who: PodiumIdentity = {
    name: gameName || activeAccount?.name || "League Client", tag: tag.slice(1) || null,
    icon: snapshot?.summoner.icon ?? activeAccount?.iconDataUrl ?? null, level: snapshot?.summoner.level ?? null,
    region, rank: rank ?? null, unranked: Boolean(snapshot),
  };
  // Connected = the client reported who is signed in (not saved stats, not an error).
  const connected = Boolean(signedIn) && !error;
  const clientProblem = !loading && !connected && isTauri();
  // The most played champion's splash is the ambient light behind every page.
  const ambient = useChampionArt(snapshot?.summaries.all?.champions[0]?.id, "splash");
  const switchingAccount = switching ? accounts?.accounts.find(account => account.id === switching.id) : undefined;
  const switchTo = (id: string) => { setPickerOpen(false); setPage("app"); setRequest({ view: "accounts", switchId: id }); };
  return <div className="fa-frame" style={ambient ? { "--ambient-art": `url("${ambient}")` } as CSSProperties : undefined}>
    <TitleBar canBack={history.index > 0} canForward={history.index < history.stack.length - 1} onBack={() => travel(-1)} onForward={() => travel(1)}
      onRefresh={() => void refresh()} refreshing={loading} champions={champions} pages={allPages.filter(p => showInNav(p, inGame))}
      onPickChampion={openChampion} onPickPage={page => go(page as Location)} snapshot={snapshot}
      onPickPlayer={riotId => go(`Player:${riotId}`)} />
    <div className="dashboard-shell">
      <aside className="sidebar dash-scope">
        <Podium who={who} connected={connected} open={pickerOpen} onToggle={() => setPickerOpen(open => !open)} crest={rank ? crestSlug(rank.tier) : null} />
        <nav aria-label="Swapper pages">
          {groups.map(group => <div className="nav-group" key={group.label} role="group" aria-label={group.label}>
            {group.items.filter(([label]) => showInNav(label, inGame)).map(([label, Icon]) => <button key={label} aria-current={current === label ? "page" : undefined} onClick={() => go(label)}>
              <Icon size={19} aria-hidden />{pageLabel(label)}
              {label === "Champion select" && inChampSelect && <span className="live-dot" title="Draft is live" />}
              {label === LIVE_PAGE && <span className="live-dot" title="A game is running" />}
            </button>)}
          </div>)}
        </nav>
        {/* Saved stats are shown as normal data (the portrait dot says the client is closed);
            the line only appears while switching or when there is nothing saved to show. */}
        {(switching || (clientProblem && !snapshot)) && <p className={`podium-status ${switching ? "is-busy" : "is-warn"}`} role="status"><i aria-hidden />
          {switching ? `Switching to ${switching.name}…` : "Open League Client for stats."}</p>}
      </aside>
      {pickerOpen && <AccountPicker accounts={accounts?.accounts ?? []} activeId={accounts?.activeId ?? null} cards={view?.cards ?? {}} disabled={Boolean(switching)}
        onSwitch={switchTo} onManage={() => { setPickerOpen(false); go("Accounts"); }} onAdd={() => { setPickerOpen(false); setPage("app"); setRequest({ view: "add" }); }} onClose={() => setPickerOpen(false)} />}
      {switching && <SwitchOverlay account={switchingAccount} name={switching.name} />}
      <OnboardingGate />
      <div className="full-app-embed" style={page === "app" ? undefined : { display: "none" }}><FlyoutApp fullApp={bridge} /></div>
      {page !== "app" && <main className="dashboard-main dash-scope">
        {selectedMatch ? <PageHeader crumbs={[{ label: matchPlayer ? 'Player profile' : "Dashboard", onClick: () => matchPlayer ? travel(-1) : go("Dashboard") }]}
            title={`${snapshot?.championNames[selectedMatch.championId] ?? championNames.get(selectedMatch.championId) ?? "Match"} · ${selectedMatch.win ? "Victory" : "Defeat"}`} />
          : <PageHeader title={playerId != null ? 'Player profile' : matchId != null ? "Match detail" : pageLabel(page)}
            actions={page === "Dashboard" && snapshot && <small className="updated">Updated {fmtAgoShort(snapshot.fetchedAt)}</small>} />}
        {error && <Notice tone="error">{error}</Notice>}
        {page === "Dashboard"
          ? snapshot ? <Dashboard snapshot={snapshot} phase={phase} onOpenLive={() => go(LIVE_PAGE)}
              onOpenChampion={(id, name) => openChampion({ id, name })}
              onOpenMatch={game => { const target = `Match:${game.id}` as const; visitedMatches.current.set(target, game); go(target); }} />
            : loading ? <DashboardSkeleton />
            : <EmptyState icon={<BrandIcon size={40} />} title="Sign in to League Client">Open League Client to see your games and rank.</EmptyState>
          : playerId != null ? <Suspense fallback={<Skeleton label="Loading player profile" rows={4} height={64} />}><PlayerPage key={playerId} riotId={playerId} snapshot={snapshot}
              names={snapshot?.championNames ?? Object.fromEntries(championNames)} onBack={() => travel(-1)}
              onOpenMatch={game => { const target = `Match:${game.id}:${encodeURIComponent(playerId)}` as const; visitedMatches.current.set(target, game); go(target); }} /></Suspense>
          : selectedMatch ? <Suspense fallback={<Skeleton label="Loading match details" rows={4} height={64} />}><MatchDetail key={page} game={selectedMatch} riotId={matchPlayer} names={snapshot?.championNames ?? Object.fromEntries(championNames)} /></Suspense>
          : page === "Live Game" ? <Suspense fallback={<Skeleton label="Loading Live Game" rows={5} height={56} />}><LiveGame championNames={championNames} /></Suspense>
          : <Suspense fallback={<Skeleton label="Loading champion tools" rows={6} height={52} />}>
              <Champions key={page} matchups={page === "Matchups"} focus={page === "Matchups" ? null : focus} />
            </Suspense>}
      </main>}
    </div>
  </div>;
}

function fmtAgoShort(at: number) {
  const minutes = Math.floor((Date.now() - at) / 60000);
  return minutes < 1 ? "just now" : minutes < 60 ? `${minutes}m ago` : `${Math.floor(minutes / 60)}h ago`;
}

createRoot(document.getElementById("root")!).render(<App />);
