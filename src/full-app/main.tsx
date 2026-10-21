import { lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { BarChart3, Crosshair, LayoutDashboard, Settings, Swords, Users } from "lucide-react";
import { BrandIcon } from "../components/BrandIcon";
import { CachedBadge } from "../runes/CachedBadge";
import { RankCrest } from "../runes/RankCrest";
import type { ChampionOption } from "../runes/types";
import type { DashboardView } from "./types";
import { Dashboard, crestSlug, splitRiotId, titleCase } from "./Dashboard";
import { TitleBar } from "./TitleBar";
import FlyoutApp from "../App";
import type { AppState, FullAppBridge, View } from "../App";
import "./dashboard.css";

const Champions = lazy(() => import("./Champions"));
type Page = "Dashboard" | "Tier list & Builds" | "Matchups";
type AppPage = "Accounts" | "Champion select" | "Settings";
type Location = Page | AppPage;
// Pages hosted by the flyout's own App component, and the App view each opens.
const appViews: Record<AppPage, View> = { Accounts: "accounts", "Champion select": "runes", Settings: "settings" };
const appPageOf = (view: View): AppPage => view === "runes" ? "Champion select" : view === "settings" ? "Settings" : "Accounts";
const groups: { label: string; items: [Location, typeof LayoutDashboard][] }[] = [
  { label: "Overview", items: [["Dashboard", LayoutDashboard]] },
  { label: "Champions", items: [["Tier list & Builds", BarChart3], ["Matchups", Swords], ["Champion select", Crosshair]] },
  { label: "Swapper", items: [["Accounts", Users], ["Settings", Settings]] },
];
const allPages = groups.flatMap(group => group.items.map(([label]) => label));
const subtitles: Record<Page, string> = {
  Dashboard: "Your recent Summoner’s Rift games, rank and form",
  "Tier list & Builds": "The strongest picks per role and rank, with builds and counters",
  Matchups: "How your champion fares into a lane opponent",
};

function App() {
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

  const bridge = useMemo<FullAppBridge>(() => ({
    request,
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
  }), [request, go]);

  const [view, setView] = useState<DashboardView | null>(null);
  const [region, setRegion] = useState<string | null>(null);
  const [champions, setChampions] = useState<ChampionOption[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const fetching = useRef(false);
  const mounted = useRef(false);
  const queued = useRef(false);
  const lastPhase = useRef("");
  async function refresh() {
    if (!isTauri()) { setError("Open Swapper from the tray to connect to League Client."); return; }
    if (fetching.current) { queued.current = true; return; }
    fetching.current = true; setLoading(true);
    void invoke<AppState>("get_state").then(state => { if (mounted.current) setRegion(state.accounts.find(a => a.id === state.activeId)?.region ?? null); }).catch(() => {});
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
      if (lastPhase.current !== payload.phase) { lastPhase.current = payload.phase; void refresh(); }
    }) : Promise.resolve(() => {});
    // Tray menu destinations (Add Account, Settings, ...) are App views.
    const navigate = isTauri() ? listen("navigate", () => setPage("app")) : Promise.resolve(() => {});
    if (isTauri()) void invoke<ChampionOption[]>("get_champion_list").then(list => { if (mounted.current) setChampions(list); }).catch(() => {});
    void refresh();
    return () => {
      mounted.current = false;
      window.removeEventListener("focus", focusRefresh);
      void navigate.then(unlisten => unlisten());
      void stop.then(unlisten => unlisten());
    };
  }, []);
  const openChampion = (champion: ChampionOption) => { setFocus({ champion }); go("Tier list & Builds"); };
  const snapshot = view?.snapshot;
  const rank = snapshot?.rank;
  const [gameName, tag] = splitRiotId(snapshot?.summoner.riotId ?? "League Client");
  return <div className="fa-frame">
    <TitleBar canBack={history.index > 0} canForward={history.index < history.stack.length - 1} onBack={() => travel(-1)} onForward={() => travel(1)}
      onRefresh={() => void refresh()} refreshing={loading} champions={champions} pages={allPages}
      onPickChampion={openChampion} onPickPage={page => go(page as Location)} />
    <div className="dashboard-shell">
      <aside className="sidebar dash-scope">
        <nav aria-label="Swapper pages">
          {groups.map(group => <div className="nav-group" key={group.label}>
            <p className="nav-label">{group.label}</p>
            {group.items.map(([label, Icon]) => <button key={label} aria-current={current === label ? "page" : undefined} onClick={() => go(label)}>
              <Icon size={17} aria-hidden />{label}
              {label === "Champion select" && inChampSelect && <span className="live-dot" title="Champion select is live" />}
            </button>)}
          </div>)}
        </nav>
        <button className="profile-card" onClick={() => go("Dashboard")} title="Open dashboard">
          <span className="profile-avatar">{snapshot?.summoner.icon ? <img src={snapshot.summoner.icon} alt="" /> : <BrandIcon size={26} />}{snapshot && <b>{snapshot.summoner.level}</b>}</span>
          <span className="profile-copy">
            <strong><span className="riot-name">{gameName}</span><span className="riot-tag">{tag}</span></strong>
            <small>{rank && <RankCrest tier={crestSlug(rank.tier)} size={16} mode="desktop" />}{rank ? `${titleCase(rank.tier)} ${rank.division} · ${rank.lp} LP` : snapshot ? "Unranked" : "Not signed in"}</small>
          </span>
        </button>
      </aside>
      <div className="full-app-embed" style={page === "app" ? undefined : { display: "none" }}><FlyoutApp fullApp={bridge} /></div>
      {page !== "app" && <main className="dashboard-main dash-scope">
        <header className="page-heading">
          <div><h1>{page}</h1><p>{subtitles[page]}</p></div>
          {page === "Dashboard" && snapshot && <small className="updated">{view?.stale ? "Showing saved stats" : `Updated ${fmtAgoShort(snapshot.fetchedAt)}`}</small>}
        </header>
        {view?.stale && page === "Dashboard" && <div className="notice" role="status">{snapshot ? <><CachedBadge stale updatedAt={snapshot.fetchedAt} /> Last saved stats for {snapshot.summoner.riotId}. </> : null}{view.message}</div>}
        {error && <div className="notice" role="alert">{error}</div>}
        {page === "Dashboard"
          ? snapshot ? <Dashboard snapshot={snapshot} region={region} onOpenChampion={(id, name) => openChampion({ id, name })} />
            : <section className="empty"><BrandIcon size={52} /><h2>{loading ? "Connecting to League Client…" : "Sign in to League Client"}</h2><p>Your recent Rift games and Solo/Duo rank appear here once League Client is open.</p></section>
          : <Suspense fallback={<p role="status" className="loading-line">Loading champion tools…</p>}>
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
