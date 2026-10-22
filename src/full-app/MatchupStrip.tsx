import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { BackIcon, ForwardIcon, SearchIcon } from "./ui/icons";
import { ChampionIcon } from "../runes/ChampionIcon";
import { RoleIcon } from "../runes/RoleIcon";
import { fmtDelta, fmtPct, type ChampionCountersView, type DuoListView } from "../runes/types";
import { useChampionArt } from "./art";
import { Notice, Skeleton } from "./ui/States";
import { fmtCount } from "./ui/format";
import { artWindow, stripRows, MIN_STRIP_GAMES } from "./buildLogic";

export function ChampionArt({ id, name, kind = "tile" }: { id: number; name: string; kind?: "tile" | "loading" }) {
  const art = useChampionArt(id, kind);
  return art ? <img className="fa-card-art" src={art} alt="" loading="lazy" /> : <ChampionIcon id={id} name={name} mode="desktop" />;
}
const laneName = (lane: string) => ({ top: "Top", jungle: "Jungle", mid: "Mid", adc: "Bot", support: "Support" } as Record<string, string>)[lane] ?? lane;
/** Card width + gap in px; must match `.fa-matchup-card` and `.fa-matchup-strip` in full-app.css. */
const CARD_STEP = 122;

export function MatchupStrip({ counters, championId, name, lane, tier, onPick }: { counters: ChampionCountersView | null; championId: number; name: string; lane: string; tier: string; onPick: (id: number) => void }) {
  const [tab, setTab] = useState("matchups");
  const [duos, setDuos] = useState<DuoListView | null>(null);
  const [error, setError] = useState("");
  const [query, setQuery] = useState("");
  // Toughest first answers "who counters this champion?".
  const [toughest, setToughest] = useState(false);
  const q = query.trim().toLowerCase();
  const reliable = stripRows(counters, toughest);
  const rows = reliable.filter(row => row.name.toLowerCase().includes(q));
  // Where the deltas cross zero: the first opponent on the other side of even.
  const flip = rows.findIndex(row => toughest ? row.delta >= 0 : row.delta < 0);
  const noisy = rows.findIndex(row => row.lowSample);
  const strip = useRef<HTMLDivElement>(null);
  useEffect(() => { if (strip.current) strip.current.scrollLeft = 0; }, [toughest]);
  const [range, setRange] = useState<[number, number]>([0, 12]);
  // Which ends of the strip have more cards behind them; drives the edge fades and arrows.
  const [edge, setEdge] = useState({ left: false, right: false });
  // Art also waits until the strip itself is near the page viewport.
  const [near, setNear] = useState(false);
  useEffect(() => {
    const node = strip.current;
    if (!node) return;
    const observer = new IntersectionObserver(entries => setNear(entries.some(entry => entry.isIntersecting)), { rootMargin: "400px 0px" });
    observer.observe(node);
    return () => observer.disconnect();
  }, [tab]);

  useEffect(() => {
    const node = strip.current;
    if (!node) return;
    let frame = 0;
    // Re-render only when the window of loaded art actually moves.
    const update = () => {
      frame = 0;
      const next = artWindow(node.scrollLeft, node.clientWidth, CARD_STEP, rows.length, 3);
      setRange(current => current[0] === next[0] && current[1] === next[1] ? current : next);
      const more = { left: node.scrollLeft > 1, right: node.scrollLeft < node.scrollWidth - node.clientWidth - 1 };
      setEdge(current => current.left === more.left && current.right === more.right ? current : more);
    };
    const onScroll = () => { if (!frame) frame = requestAnimationFrame(update); };
    // A vertical wheel scrolls the strip sideways until it reaches an end, then the page.
    const onWheel = (event: WheelEvent) => {
      if (Math.abs(event.deltaY) <= Math.abs(event.deltaX)) return;
      const max = node.scrollWidth - node.clientWidth;
      if ((event.deltaY < 0 && node.scrollLeft <= 0) || (event.deltaY > 0 && node.scrollLeft >= max - 1)) return;
      event.preventDefault();
      // Instant, not smooth: a new smooth scroll per wheel tick cancels the last one and crawls.
      node.scrollLeft += event.deltaY * (event.deltaMode === 1 ? 40 : 2.5);
    };
    update();
    node.addEventListener("scroll", onScroll, { passive: true });
    node.addEventListener("wheel", onWheel, { passive: false });
    const resize = new ResizeObserver(onScroll);
    resize.observe(node);
    return () => { cancelAnimationFrame(frame); node.removeEventListener("scroll", onScroll); node.removeEventListener("wheel", onWheel); resize.disconnect(); };
  }, [rows.length, tab]);

  const slide = (direction: -1 | 1) => strip.current?.scrollBy({ left: direction * strip.current.clientWidth * 0.8, behavior: "smooth" });
  const onKey = (event: KeyboardEvent<HTMLDivElement>) => {
    if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
    const cards = Array.from(event.currentTarget.querySelectorAll<HTMLButtonElement>(".fa-matchup-card"));
    if (!cards.length) return;
    event.preventDefault();
    const index = cards.indexOf(document.activeElement as HTMLButtonElement);
    const next = event.key === "Home" ? 0 : event.key === "End" ? cards.length - 1 : Math.min(cards.length - 1, Math.max(0, index + (event.key === "ArrowLeft" ? -1 : 1)));
    cards[next].focus({ preventScroll: false });
  };

  useEffect(() => {
    if (tab !== "synergies") return;
    let live = true; setDuos(null); setError("");
    void invoke<DuoListView>("get_duo_list", { tier }).then(value => { if (live) setDuos(value); }).catch(reason => { if (live) setError(String(reason)); });
    return () => { live = false; };
  }, [tab, tier]);

  return <section className="fa-card fa-matchups">
    <div className="fa-section-heading">
      <div><h3>Matchups as {name}</h3><p>{counters ? `${reliable.length} ${laneName(lane).toLowerCase()} opponents with ${MIN_STRIP_GAMES}+ games, ` : ""}{toughest ? "toughest first" : "best first"} · win rate versus {name}’s average</p></div>
      <div className="fa-actions">
        {tab === "matchups" && <div className="fa-seg" role="group" aria-label="Order"><button aria-pressed={!toughest} onClick={() => setToughest(false)}>Best</button><button aria-pressed={toughest} onClick={() => setToughest(true)}>Toughest</button></div>}
        {tab === "matchups" && <label className="fa-strip-search"><SearchIcon size={14} aria-hidden /><input aria-label="Find opponent" placeholder="Find opponent" value={query} onChange={event => setQuery(event.target.value)} /></label>}
        {["adc", "support"].includes(lane) && <div className="fa-seg"><button aria-pressed={tab === "matchups"} onClick={() => setTab("matchups")}>Matchups</button><button aria-pressed={tab === "synergies"} onClick={() => setTab("synergies")}>Synergies</button></div>}
      </div>
    </div>
    {tab === "matchups" ? <>
      <div className="fa-strip-frame" data-left={edge.left} data-right={edge.right}>
      <button className="fa-strip-arrow is-left" aria-label="Scroll matchups left" tabIndex={-1} onClick={() => slide(-1)}><BackIcon size={20} /></button>
      <button className="fa-strip-arrow is-right" aria-label="Scroll matchups right" tabIndex={-1} onClick={() => slide(1)}><ForwardIcon size={20} /></button>
      <div ref={strip} className="fa-matchup-strip" role="list" aria-label={`Matchups as ${name}, ${toughest ? "toughest" : "best"} first. Use the arrow keys to move between opponents.`} onKeyDown={onKey}>
        {rows.map((row, index) => <div role="listitem" className={`fa-matchup-slot${index === flip && index > 0 ? " is-flip" : ""}${index === noisy && index > 0 ? " is-noisy" : ""}`} data-label={index === noisy && index > 0 ? "Small sample" : undefined} key={row.championId}>
          <button className={`fa-matchup-card${row.lowSample ? " is-low-sample" : ""} ${row.delta >= 0 ? "is-good" : "is-bad"}`} onClick={() => onPick(row.championId)}
            title={`${row.name}: ${fmtPct(row.winPct)} win rate over ${fmtCount(row.games)} games this patch${row.prevGames ? ` and ${fmtCount(row.prevGames)} last patch` : ""}${row.lowSample ? " (small sample)" : ""}`}>
            <span className="fa-art-wrap">{near && index >= range[0] && index <= range[1] && <ChampionArt id={row.championId} name={row.name} />}</span>
            <span className="fa-matchup-copy"><strong>{row.name}</strong><b className={row.delta >= 0 ? "up" : "down"}>{fmtDelta(row.delta)}<i>%</i></b><small>{fmtCount(row.games + (row.prevGames ?? 0))} games</small><span className="fa-lane-tag"><RoleIcon role={lane} size={12} mode="desktop" />{row.lowSample ? "Low sample" : laneName(lane)}</span></span>
          </button>
        </div>)}
      </div>
      </div>
      {!rows.length && (!counters ? <Skeleton label="Loading matchups" rows={1} height={228} /> : <p className="fa-panel-empty">{q ? `No opponent matches “${query}”.` : `No opponent with ${MIN_STRIP_GAMES}+ games for this role and rank yet.`}</p>)}
    </> : <div className="fa-synergy-grid">{error && <Notice tone="error">{error}</Notice>}{!duos && !error && <Skeleton label="Loading bot-lane pairs" rows={2} height={64} />}{duos && duos.rows.filter(row => row.adcId === championId || row.supportId === championId).map(row => {
      const id = row.adcId === championId ? row.supportId : row.adcId;
      const partner = row.adcId === championId ? row.supportName : row.adcName;
      return <article className="fa-synergy" key={id}><ChampionIcon id={id} name={partner} mode="desktop" /><strong>{partner}</strong><b>{fmtPct(row.winPct)}</b><small>{fmtCount(row.games)} games together</small></article>;
    })}{duos && !duos.rows.some(row => row.adcId === championId || row.supportId === championId) && <p>No bot-lane pair data for this champion.</p>}</div>}
  </section>;
}
