import { Fragment, useEffect, useRef, useState, type CSSProperties } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { ChevronDown } from "lucide-react";
import type { Account } from "../App";
import { ChampionIcon } from "../runes/ChampionIcon";
import { ItemIcon } from "../runes/ItemIcon";
import { RankCrest } from "../runes/RankCrest";
import { RoleIcon } from "../runes/RoleIcon";
import { RuneIcon } from "../runes/RuneIcon";
import { SpellIcon } from "../runes/SpellIcon";
import { fmtAgo } from "../runes/types";
import { useChampionArt } from "./art";
import { benchKey, defaultQueue, formatStat, insights, MIN_COMPARED, opponent, tierLabel, tiles } from "./dashboardModel";
import type { AccountCard, Bench, Game, Player, Snapshot } from "./types";
export const queueLabels: Record<string, string> = { all: "All", solo: "Solo/Duo", flex: "Flex", normal: "Normal" };
const roleLabels: Record<string, string> = { top: "Top", jungle: "Jungle", mid: "Mid", adc: "Bot", support: "Support" };

export { crestForTier as crestSlug } from "./buildLogic";
import { crestForTier as crestSlug } from "./buildLogic";

export function titleCase(text: string) {
  return text.charAt(0) + text.slice(1).toLowerCase();
}

export function splitRiotId(riotId: string): [string, string] {
  const at = riotId.lastIndexOf("#");
  return at > 0 ? [riotId.slice(0, at), riotId.slice(at)] : [riotId, ""];
}

/** True once the element has come within a screen of the viewport. Match rows
 *  load their ~10 icons only then, so opening the Dashboard decodes a third
 *  of them (each icon is a data URL held by the page). */
function useSeen<T extends Element>() {
  const ref = useRef<T>(null);
  const [seen, setSeen] = useState(false);
  useEffect(() => {
    const node = ref.current;
    if (!node || seen) return;
    const observer = new IntersectionObserver(entries => {
      if (entries.some(entry => entry.isIntersecting)) { setSeen(true); observer.disconnect(); }
    }, { rootMargin: "200px 0px" });
    observer.observe(node);
    return () => observer.disconnect();
  }, [seen]);
  return [ref, seen] as const;
}

const pct = (wins: number, games: number) => (games ? Math.round(100 * wins / games) : 0);
const clock = (seconds: number) => `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
const signed = (value: number, digits: number, unit = "") => `${value >= 0 ? "+" : "−"}${formatStat(Math.abs(value), digits, unit)}`;

type Props = {
  snapshot: Snapshot; region: string | null; accounts: Account[]; activeId: string | null;
  cards: Record<string, AccountCard>; phase: string; busy: boolean;
  onSwitch: (id: string) => void; onOpenLive: () => void; onOpenChampion: (id: number, name: string) => void;
};

export function Dashboard(props: Props) {
  const { snapshot, onOpenChampion } = props;
  const [queue, setQueue] = useState(() => defaultQueue(snapshot.games));
  const [benches, setBenches] = useState<Record<string, Bench>>({});
  const games = snapshot.games.filter(game => queue === "all" || game.queue === queue);
  const summary = snapshot.summaries[queue];
  const name = (id: number) => snapshot.championNames[id] ?? `Champion ${id}`;
  const tier = snapshot.rank?.tier ?? "";
  const picks = [...new Set(snapshot.games.filter(g => g.role).map(benchKey))].sort().join(",");
  useEffect(() => {
    if (!isTauri() || !picks) return;
    let live = true;
    const list = picks.split(",").map(key => { const [id, role] = key.split("|"); return [Number(id), role]; });
    invoke<Record<string, Bench>>("dashboard_benchmarks", { picks: list, tier }).then(next => { if (live) setBenches(next); }).catch(() => {});
    return () => { live = false; };
  }, [picks, tier]);
  return <div className="dash-v2">
    <TopStrip {...props} featured={summary.champions[0]?.id ?? games[0]?.championId} />
    <Performance games={games} queue={queue} onQueue={setQueue} benches={benches} tier={tier} name={name} />
    <div className="dash-columns">
      <section className="panel matches-panel" aria-labelledby="matches-title">
        <div className="panel-head"><h2 id="matches-title">Match history</h2><small>{games.length} {games.length === 1 ? "game" : "games"} · {queue === "all" ? "all queues" : queueLabels[queue]}</small></div>
        {!games.length ? <p className="panel-empty">No {queueLabels[queue]} games in your latest 20 matches.</p>
          : <ol className="match-list">{games.map(game => <MatchRow key={game.id} game={game} name={name} onOpen={() => onOpenChampion(game.championId, name(game.championId))} />)}</ol>}
      </section>
      <section className="panel" aria-labelledby="champions-title">
        <div className="panel-head"><h2 id="champions-title">Your champions &amp; roles</h2><small>{queueLabels[queue]}</small></div>
        {!summary.champions.length ? <p className="panel-empty">Play a game to see champion results.</p> : <ul className="champ-perf">
          {summary.champions.map(c => {
            const total = c.wins + c.losses;
            return <li key={c.id}><button onClick={() => onOpenChampion(c.id, name(c.id))} title={`Open ${name(c.id)} build`}>
              <ChampionIcon id={c.id} name={name(c.id)} mode="desktop" />
              <span className="champ-perf-copy"><strong>{name(c.id)}</strong><small>{c.kda.toFixed(2)} KDA · {total} {total === 1 ? "game" : "games"}</small></span>
              <span className="champ-perf-rate"><b className={pct(c.wins, total) >= 50 ? "up" : "down"}>{pct(c.wins, total)}%</b><small>{c.wins}W {c.losses}L</small><i style={{ "--rate": `${pct(c.wins, total)}%` } as CSSProperties} aria-hidden /></span>
            </button></li>;
          })}
        </ul>}
        <div className="role-strip" aria-label="Win rate by role (estimated from spells, items and lane)">
          {summary.roles.map(r => <div key={r.role} className={r.games ? "" : "is-empty"} title={`${roleLabels[r.role]}: ${r.wins}W ${r.games - r.wins}L`}>
            <RoleIcon role={r.role} size={18} mode="desktop" />
            <b className={!r.games ? "" : pct(r.wins, r.games) >= 50 ? "up" : "down"}>{r.games ? `${pct(r.wins, r.games)}%` : "–"}</b>
            <small>{r.games} {r.games === 1 ? "game" : "games"}</small>
          </div>)}
        </div>
      </section>
    </div>
  </div>;
}

const phaseLabels: Record<string, string> = {
  Lobby: "In lobby", Matchmaking: "In queue", ReadyCheck: "Match found", ChampSelect: "In champion select",
  GameStart: "Game loading", InProgress: "In game", Reconnect: "In game", WaitingForStats: "Game ending",
  PreEndOfGame: "Game ending", EndOfGame: "Post-game", None: "Online",
};
const inGamePhases = new Set(["GameStart", "InProgress", "Reconnect"]);

function TopStrip({ snapshot, region, accounts, activeId, cards, phase, busy, onSwitch, onOpenLive, featured }: Props & { featured: number | undefined }) {
  const splash = useChampionArt(featured, "splash");
  const [gameName, tag] = splitRiotId(snapshot.summoner.riotId);
  const rank = snapshot.rank;
  const inGame = inGamePhases.has(phase);
  const others = accounts.filter(a => a.id !== activeId);
  return <section className="top-strip" aria-label="Accounts">
    <div className={`active-account${inGame ? " is-live" : ""}`}>
      {splash && <img className="active-art" src={splash} alt="" draggable={false} />}
      <div className="active-avatar">{snapshot.summoner.icon ? <img src={snapshot.summoner.icon} alt="" /> : <span aria-hidden />}<b title="Summoner level">{snapshot.summoner.level}</b></div>
      <div className="active-copy">
        <h2><span className="riot-name">{gameName}</span><span className="riot-tag">{tag}</span></h2>
        <p className="active-rank">{rank ? <><RankCrest tier={crestSlug(rank.tier)} size={22} mode="desktop" /><strong>{`${titleCase(rank.tier)} ${rank.division}`.trim()}</strong><span>{rank.lp} LP · {rank.wins}W {rank.losses}L</span></> : <strong>Unranked</strong>}</p>
        <p className="active-status">{region && <span className="chip">{region}</span>}<span className={`status-dot${phase ? " is-on" : ""}`} />{phase ? phaseLabels[phase] ?? "Online" : "Status unknown"}</p>
      </div>
      {inGame && <button className="fa-primary live-button" onClick={onOpenLive}>Open Live Game</button>}
    </div>
    {others.length > 0 && <ul className="other-accounts">{others.map(account => {
      const card = cards[account.id];
      const total = card ? card.wins + card.losses : 0;
      return <li key={account.id} className="other-account">
        <div className="other-head">{account.iconDataUrl ? <img src={account.iconDataUrl} alt="" loading="lazy" /> : <span className="other-initial">{account.name[0]?.toUpperCase()}</span>}
          <span><strong>{account.name}</strong><small>{card?.rank ? `${titleCase(card.rank.tier)} ${card.rank.division} · ${card.rank.lp} LP` : card ? "Unranked" : account.riotId ?? ""}</small></span></div>
        <p className="other-meta">{card ? <>{card.lastPlayed ? `Played ${fmtAgo(card.lastPlayed)}` : "No recent games"}{total > 0 && <> · <b className={pct(card.wins, total) >= 50 ? "up" : "down"}>{card.wins}W {card.losses}L</b></>}</> : "Sign in once to see stats"}</p>
        <button className="fa-secondary" disabled={busy} onClick={() => onSwitch(account.id)}>Switch</button>
      </li>;
    })}</ul>}
  </section>;
}

function Performance({ games, queue, onQueue, benches, tier, name }: {
  games: Game[]; queue: string; onQueue: (queue: string) => void; benches: Record<string, Bench>; tier: string; name: (id: number) => string;
}) {
  const rows = tiles(games, benches, tierLabel(tier), name);
  const lines = games.length >= 5 ? insights(games, name) : [];
  const oldestFirst = [...games].reverse();
  const wins = games.filter(g => g.win).length;
  return <section className="panel performance" aria-labelledby="performance-title">
    <div className="performance-head">
      <h2 id="performance-title">Performance</h2>
      <div className="fa-seg" role="group" aria-label="Queue">
        {Object.entries(queueLabels).map(([value, label]) => <button key={value} aria-pressed={queue === value} onClick={() => onQueue(value)}>{label}</button>)}
      </div>
      <div className="wl-strip" aria-label={`${wins} wins, ${games.length - wins} losses, oldest first`}>
        {oldestFirst.map(g => <i key={g.id} className={g.win ? "is-win" : "is-loss"} title={`${g.win ? "Win" : "Loss"} · ${name(g.championId)} · ${g.kills}/${g.deaths}/${g.assists} · ${fmtAgo(g.createdAt)}`} />)}
        <small>{wins}W {games.length - wins}L</small>
      </div>
    </div>
    <div className="tiles">{rows.map(tile => <article key={tile.key} className="tile">
      <small>{tile.label}</small>
      <strong>{tile.value == null ? "–" : formatStat(tile.value, tile.digits, tile.unit)}</strong>
      {tile.versus ? <p title={`Your average and theirs over the same ${tile.versus.games} games`}>vs {tile.versus.label} {formatStat(tile.versus.value, tile.digits, tile.unit)} <b className={tile.versus.delta >= 0 ? "up" : "down"}>{signed(tile.versus.delta, tile.digits, tile.unit)}</b></p>
        : <p>{games.length < MIN_COMPARED ? `${games.length} ${games.length === 1 ? "game" : "games"}` : "Not enough comparable games"}</p>}
    </article>)}</div>
    {lines.length > 0 && <ul className="insights">{lines.map(line => <li key={line}>{line}</li>)}</ul>}
  </section>;
}

function MatchRow({ game, name, onOpen }: { game: Game; name: (id: number) => string; onOpen: () => void }) {
  const ratio = (game.kills + game.assists) / Math.max(1, game.deaths);
  const [row, seen] = useSeen<HTMLLIElement>();
  const [open, setOpen] = useState(false);
  const opp = opponent(game);
  const csDiff = opp ? game.cs - opp.cs : null;
  return <li ref={row} className={`match-row ${game.win ? "is-win" : "is-loss"}${open ? " is-open" : ""}`}>
    <div className="match-meta">
      <strong>{game.win ? "Victory" : "Defeat"}</strong>
      <span>{queueLabels[game.queue]}{game.lpChange != null &&
        <b className={game.lpChange >= 0 ? "up" : "down"}> {game.lpChange >= 0 ? "+" : "−"}{Math.abs(game.lpChange)} LP</b>}</span>
      <small>{fmtAgo(game.createdAt)} · {clock(game.duration)}</small>
    </div>
    <button className="match-champ" onClick={onOpen} title={`Open ${name(game.championId)} build`}>
      <ChampionIcon id={game.championId} name={name(game.championId)} mode="desktop" />
      {game.level > 0 && <b>{game.level}</b>}
    </button>
    <div className="match-loadout" aria-label="Spells and keystone">
      {seen && game.spells.map((id, i) => <SpellIcon key={i} id={id} mode="desktop" />)}
      {seen && game.keystone > 0 && <RuneIcon id={game.keystone} mode="desktop" className="match-keystone" />}
    </div>
    <div className="match-kda">
      <strong>{game.kills} <i>/</i> <em>{game.deaths}</em> <i>/</i> {game.assists}</strong>
      <small className={ratio >= 3 ? "up" : ratio < 1.5 ? "down" : ""}>{ratio.toFixed(2)} KDA</small>
    </div>
    <div className="match-cs"><strong>{game.cs || "–"}</strong><small>{game.metrics[2] == null ? "CS" : `${game.metrics[2].toFixed(1)} / min`}</small></div>
    <div className="match-opp" title={opp ? `Lane opponent: ${name(opp.championId)}` : "Lane opponent unclear"}>
      {opp ? <>{seen && <ChampionIcon id={opp.championId} name={name(opp.championId)} mode="desktop" />}<span><small>vs {name(opp.championId)}</small><b className={csDiff! >= 0 ? "up" : "down"}>{csDiff! >= 0 ? "+" : "−"}{Math.abs(csDiff!)} CS</b></span></> : <small>–</small>}
    </div>
    <div className="match-items" aria-label="Items">
      {!game.items.length ? <small>Refresh to load items</small>
        : game.items.map((id, i) => seen ? <ItemIcon key={i} id={id} mode="desktop" className={i === 6 ? "is-trinket" : undefined} /> : <span key={i} className={i === 6 ? "is-trinket" : undefined} />)}
    </div>
    <div className={`placement${game.placement === 1 ? " is-mvp" : ""}`} title="Rank among all 10 players by KDA, damage, CS and vision">
      {game.placement ? <><b>#{game.placement}</b><small>{game.placement === 1 ? "MVP" : "place"}</small></> : <small>–</small>}
    </div>
    <button className="match-expand" aria-expanded={open} disabled={!game.players.length} onClick={() => setOpen(!open)} title={game.players.length ? "Show all players" : "Refresh to load players"}><ChevronDown size={16} /></button>
    {open && <MatchPlayers game={game} name={name} />}
  </li>;
}

function MatchPlayers({ game, name }: { game: Game; name: (id: number) => string }) {
  const teams: [string, Player[]][] = [[game.win ? "Your team · Victory" : "Your team · Defeat", game.players.filter(p => p.ally)], [game.win ? "Enemy team · Defeat" : "Enemy team · Victory", game.players.filter(p => !p.ally)]];
  return <div className="match-players">{teams.map(([label, players], t) => <Fragment key={label}>
    <p className={`match-team ${(t === 0) === game.win ? "up" : "down"}`}>{label}</p>
    {players.map((p, i) => <div key={i} className={`match-player${p.me ? " is-me" : ""}`}>
      <ChampionIcon id={p.championId} name={name(p.championId)} mode="desktop" />
      <span className="player-name">{p.riotId || name(p.championId)}</span>
      <span>{p.kills}/{p.deaths}/{p.assists}</span>
      <span>{p.damage.toLocaleString("en-US")} dmg</span>
      <span>{p.cs} CS</span>
      <span className="player-items">{p.items.map((id, j) => <ItemIcon key={j} id={id} mode="desktop" />)}</span>
    </div>)}
  </Fragment>)}</div>;
}
