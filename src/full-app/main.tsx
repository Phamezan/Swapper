import { lazy, Suspense, useEffect, useMemo, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { BrandIcon } from "../components/BrandIcon";
import { ChampionIcon } from "../runes/ChampionIcon";
import { CachedBadge } from "../runes/CachedBadge";
import { fmtAgo } from "../runes/types";
import type { DashboardView, Snapshot } from "./types";
import { sparkline } from "./sparkline";
import FlyoutApp from "../App";
import type { FullAppBridge, View } from "../App";
import "./dashboard.css";

const Champions = lazy(() => import("./Champions"));
const pages = ["Dashboard", "Tier list & Builds", "Matchups"] as const;
type Page = typeof pages[number];
// Pages hosted by the flyout's own App component, and the App view each opens.
const appPages = [["Accounts", "accounts", "@"], ["Champion select", "runes", "◎"], ["Settings", "settings", "⚙"]] as const;
type AppPage = typeof appPages[number][0];
const appPageOf = (view: View): AppPage => view === "runes" ? "Champion select" : view === "settings" ? "Settings" : "Accounts";
const metricLabels = ["KDA", "Kill participation", "CS / min", "Damage / min", "Damage share", "Vision / min"];
const queueLabels: Record<string, string> = { all: "All", solo: "Solo", flex: "Flex", normal: "Normal" };

function App() {
  const [page, setPage] = useState<Page | "app">("Dashboard");
  const [appView, setAppView] = useState<View>("accounts");
  const [request, setRequest] = useState<FullAppBridge["request"]>(null);
  const current: Page | AppPage = page === "app" ? appPageOf(appView) : page;
  const currentRef = useRef(current);
  currentRef.current = current;
  // Where champion select interrupted the user; restored when it ends.
  const returnTo = useRef<Page | AppPage | null>(null);
  function go(target: Page | AppPage) {
    const app = appPages.find(([label]) => label === target);
    if (app) { setPage("app"); setRequest({ view: app[1] }); }
    else setPage(target as Page);
  }
  const bridge = useMemo<FullAppBridge>(() => ({
    request,
    onViewChange: setAppView,
    onChampSelectChange: active => {
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
  }), [request]);
  const [view, setView] = useState<DashboardView | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const fetching = useRef(false);
  const mounted = useRef(false);
  const queued = useRef(false);
  const lastPhase = useRef("");
  async function refresh() {
    if (!isTauri()) { setError("Open this dashboard from the Swapper tray app to connect to League Client."); return; }
    if (fetching.current) { queued.current = true; return; }
    fetching.current = true; setLoading(true);
    try { const next = await invoke<DashboardView>("dashboard_snapshot"); if (mounted.current) { setView(next); setError(""); } }
    catch (reason) { if (mounted.current) setError(String(reason)); }
    finally {
      fetching.current = false;
      if (mounted.current) { setLoading(false); if (queued.current) { queued.current = false; void refresh(); } }
    }
  }
  useEffect(() => {
    mounted.current = true;
    const focus = () => { if (document.visibilityState === "visible") void refresh(); };
    window.addEventListener("focus", focus);
    const stop = isTauri() ? listen<{ phase: string }>("champ_select", ({ payload }) => {
      if (lastPhase.current !== payload.phase) { lastPhase.current = payload.phase; void refresh(); }
    }) : Promise.resolve(() => {});
    // Tray menu destinations (Add Account, Settings, ...) are App views.
    const navigate = isTauri() ? listen("navigate", () => setPage("app")) : Promise.resolve(() => {});
    void refresh();
    return () => {
      void navigate.then(unlisten => unlisten()); mounted.current = false; window.removeEventListener("focus", focus); void stop.then(unlisten => unlisten()); };
  }, []);
  const snapshot = view?.snapshot;
  const rank = snapshot?.rank;
  return <div className="dashboard-shell">
    <aside className="sidebar dash-scope">
      <div className="brand"><BrandIcon size={34} /><strong>Swapper</strong><small>LEAGUE COMPANION</small></div>
      <nav aria-label="Swapper pages">
        {pages.map((label, i) => <button key={label} aria-current={current === label ? "page" : undefined} onClick={() => go(label)}><span aria-hidden>{["◫", "≡", "⇄"][i]}</span>{label}</button>)}
        {appPages.map(([label, , icon]) => <button key={label} aria-current={current === label ? "page" : undefined} onClick={() => go(label)}><span aria-hidden>{icon}</span>{label}</button>)}
      </nav>
      <div className="sidebar-foot"><Avatar snapshot={snapshot} /><div><strong>{snapshot?.summoner.riotId ?? "League Client"}</strong><small>{rank ? `${rank.tier} ${rank.division} · ${rank.lp} LP` : "Unranked"}</small></div></div>
    </aside>
    <div className="full-app-embed" style={page === "app" ? undefined : { display: "none" }}><FlyoutApp fullApp={bridge} /></div>
    {page !== "app" && <main className="dashboard-main dash-scope">
      <header className="page-heading"><div><p className="eyebrow">SUMMONER’S RIFT</p><h1>{page}</h1></div><div className="refresh"><small>{snapshot ? `Updated ${fmtAgo(snapshot.fetchedAt)}` : "Signed-in account only"}</small><button onClick={() => void refresh()} disabled={loading}>{loading ? "Refreshing…" : "Refresh"}</button></div></header>
      {view?.stale && <div className="notice" role="status">{snapshot ? <><CachedBadge stale updatedAt={snapshot.fetchedAt} /> Last snapshot of {snapshot.summoner.riotId}. </> : null}{view.message}</div>}
      {error && <div className="notice" role="alert">{error}</div>}
      {page === "Dashboard" ? snapshot ? <Overview snapshot={snapshot} /> : <section className="empty"><BrandIcon size={52} /><h2>{loading ? "Connecting to League Client…" : "Your next game starts here"}</h2><p>Sign in to League Client to see your recent Rift games and Solo/Duo rank.</p></section>
        : <Suspense fallback={<p role="status">Loading champion tools…</p>}><Champions key={page} matchups={page === "Matchups"} /></Suspense>}
    </main>}
  </div>;
}

function Avatar({ snapshot }: { snapshot: Snapshot | null | undefined }) {
  return snapshot?.summoner.icon ? <img className="summoner-icon" src={snapshot.summoner.icon} alt="" /> : <span className="summoner-icon avatar-placeholder" aria-hidden>◇</span>;
}

function Overview({ snapshot }: { snapshot: Snapshot }) {
  const [queue, setQueue] = useState("all");
  const { summoner, rank } = snapshot;
  const games = snapshot.games.filter(game => queue === "all" || game.queue === queue);
  const summary = snapshot.summaries[queue];
  return <>
    <section className="summoner-header"><Avatar snapshot={snapshot} /><div><p className="eyebrow">LEVEL {summoner.level}</p><h2>{summoner.riotId}</h2><p>Your last {snapshot.games.length} eligible games from the latest 20 matches</p></div><div className="rank-summary"><small>RANKED SOLO / DUO</small><strong>{rank ? `${rank.tier} ${rank.division}` : "Unranked"}</strong><p>{rank ? `${rank.lp} LP · ${rank.wins}W – ${rank.losses}L` : "Play ranked to establish your rank"}</p></div></section>
    <div className="section-heading"><h2>Recent performance</h2><div className="filters" aria-label="Queue filter">{Object.entries(queueLabels).map(([value,label]) => <button key={value} aria-pressed={queue === value} onClick={() => setQueue(value)}>{label}</button>)}</div></div>
    <div className="metrics">{summary.metrics.map((metric,i) => <article className="metric-card" key={metricLabels[i]}><small>{metricLabels[i]}</small><strong>{metric.average === null ? "—" : `${metric.average.toFixed(i === 3 ? 0 : 2)}${i === 1 || i === 4 ? "%" : ""}`}</strong><svg viewBox="0 0 100 28" aria-label={`${metricLabels[i]} trend, oldest to newest`} role="img"><polyline fill="none" stroke="currentColor" strokeWidth="1.7" points={sparkline(metric.points)} /></svg><span className={metric.delta !== null && metric.delta < 0 ? "down" : "up"}>{metric.delta === null ? "Team data unavailable" : `${metric.delta >= 0 ? "+" : ""}${metric.delta.toFixed(2)} vs sample average`}</span></article>)}</div>
    <p className="sample-note">Averages per game · delta compares the latest 5 games with this queue’s sample average · trend: latest 10. Remakes excluded.</p>
    <div className="section-heading"><h2>Champion performance</h2><small>{games.length} games</small></div>
    <section className="champion-strip" aria-label="Champion performance">{summary.champions.map(champion => <article key={champion.id}><ChampionIcon id={champion.id} name={snapshot.championNames[champion.id]} mode="desktop" /><div><strong>{snapshot.championNames[champion.id] ?? `Champion ${champion.id}`}</strong><small>{champion.wins}W – {champion.losses}L</small><small>{champion.kda.toFixed(2)} KDA</small></div></article>)}</section>
    <div className="section-heading"><h2>Recent matches</h2><small>Summoner’s Rift</small></div>
    <section className="matches" aria-label="Recent matches">{!games.length && <p className="empty">No recent games for this queue.</p>}{games.map(game => <article className={`match ${game.win ? "win" : "loss"}`} key={game.id}><strong className="result">{game.win ? "Victory" : "Defeat"}</strong><ChampionIcon id={game.championId} name={snapshot.championNames[game.championId]} mode="desktop" /><div className="match-queue"><strong>{snapshot.championNames[game.championId] ?? `Champion ${game.championId}`} · {queueLabels[game.queue]}</strong><small>{fmtAgo(game.createdAt)}</small></div><strong>{game.kills} / {game.deaths} / {game.assists}</strong><span>{game.metrics[0]?.toFixed(2)} KDA</span><small>{Math.floor(game.duration / 60)}:{String(game.duration % 60).padStart(2,"0")}</small></article>)}</section>
  </>;
}

createRoot(document.getElementById("root")!).render(<App />);
