import { useEffect, useRef, type CSSProperties } from "react";
import { X } from "lucide-react";
import { ChampionIcon } from "../runes/ChampionIcon";
import { RankCrest } from "../runes/RankCrest";
import { RoleIcon } from "../runes/RoleIcon";
import { fmtAgo } from "../runes/types";
import { useChampionArt } from "./art";
import { CooldownSheet } from "./CooldownSheet";
import { clock, crestSlug, kda, pct, rankLabel, splitRiotId, type Player, type Rank } from "./LiveGameModel";

const queueNames: Record<string, string> = { solo: "Solo/Duo", flex: "Flex", normal: "Normal" };

/** A compact profile of one player in the game: rank cards and the recent
 *  games already fetched for the Live Game page, and for an enemy their
 *  ability and summoner spell cooldowns (one local game-data read). */
export function LiveGamePlayer({ player: p, championNames, enemy, onClose }: {
  player: Player; championNames: Map<number, string>; enemy: boolean; onClose: () => void;
}) {
  const art = useChampionArt(p.championId, "tile");
  const panel = useRef<HTMLElement>(null);
  useEffect(() => {
    panel.current?.focus();
    const close = (event: KeyboardEvent) => { if (event.key === "Escape") onClose(); };
    window.addEventListener("keydown", close);
    return () => window.removeEventListener("keydown", close);
  }, [onClose]);
  const [name, tag] = splitRiotId(p.riotId || p.championName);
  const h = p.history;
  return <div className="lg-scrim" onClick={onClose}>
    <aside ref={panel} tabIndex={-1} className="lg-profile" role="dialog" aria-modal="true" aria-label={`${name} profile`} onClick={event => event.stopPropagation()}>
      <header className="lg-profile-head">
        {art && <img src={art} alt="" draggable={false} />}
        <button className="lg-close" onClick={onClose} aria-label="Close profile"><X size={16} /></button>
        <ChampionIcon id={p.championId} name={p.championName} mode="desktop" />
        <div>
          <h2><span className="riot-name">{name}</span><span className="riot-tag">{tag}</span></h2>
          <p>{p.championName}{p.summonerLevel ? ` · Level ${p.summonerLevel}` : ""}{p.isSelf ? " · You" : ""}</p>
        </div>
      </header>
      <div className="lg-rank-cards">
        <RankCard label="Ranked Solo/Duo" rank={p.solo} />
        <RankCard label="Ranked Flex" rank={p.flex} />
      </div>
      {enemy && <CooldownSheet championId={p.championId} spells={p.spells} />}
      {h && h.form.games > 0 && <dl className="lg-facts">
        <div><dt>Latest {h.form.games}</dt><dd className={pct(h.form.wins, h.form.games) >= 50 ? "up" : "down"}>{pct(h.form.wins, h.form.games)}%</dd></div>
        <div><dt>KDA</dt><dd>{kda(h.form).toFixed(2)}</dd></div>
        <div><dt>{p.championName || "Champion"}</dt><dd>{h.onChampion.games ? `${pct(h.onChampion.wins, h.onChampion.games)}% · ${h.onChampion.games}` : "–"}</dd></div>
        <div><dt>Main role</dt><dd>{h.mainRole ? <><RoleIcon role={h.mainRole[0]} size={16} mode="desktop" /> {h.mainRole[1]}/{h.form.games}</> : "–"}</dd></div>
      </dl>}
      {p.badges.length > 0 && <ul className="lg-badge-list">{p.badges.map(b => <li key={b.kind}><span className={`lg-badge is-${b.kind}`}>{b.label}</span>{b.detail}</li>)}</ul>}
      <h3>Recent Rift games</h3>
      {!h ? <p className="panel-empty">{p.status === "pending" ? "Loading…" : "Match history is unavailable for this player."}</p>
        : !h.recent.length ? <p className="panel-empty">No Summoner's Rift games in the latest 20 matches.</p>
        : <ol className="lg-recent">{h.recent.map(g => <li key={g.createdAt} className={g.win ? "is-win" : "is-loss"}>
            <ChampionIcon id={g.championId} name={championNames.get(g.championId)} mode="desktop" />
            <span><strong>{g.win ? "Win" : "Loss"}</strong><small>{queueNames[g.queue] ?? g.queue} · {fmtAgo(g.createdAt)}</small></span>
            <span><strong>{g.kills} <i>/</i> <em>{g.deaths}</em> <i>/</i> {g.assists}</strong><small>{kda(g).toFixed(2)} KDA</small></span>
            <span><strong>{g.cs}</strong><small>CS · {clock(g.duration)}</small></span>
          </li>)}</ol>}
    </aside>
  </div>;
}

function RankCard({ label, rank }: { label: string; rank: Rank | null }) {
  const games = rank ? rank.wins + rank.losses : 0;
  return <section className={`lg-rank-card${rank ? "" : " is-unranked"}`}>
    <RankCrest tier={crestSlug(rank?.tier)} size={44} mode="desktop" />
    <div>
      <small>{label}</small>
      <strong>{rankLabel(rank)}</strong>
      {rank && <p><b>{rank.lp} LP</b> {rank.wins}W {rank.losses}L · {pct(rank.wins, games)}%</p>}
    </div>
    {rank && <i style={{ "--rate": `${pct(rank.wins, games)}%` } as CSSProperties} aria-hidden />}
  </section>;
}
