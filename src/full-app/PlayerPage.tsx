import { useEffect, useId, useMemo, useState, type CSSProperties } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { ArrowDownRight, ArrowUpRight, Crosshair, Eye, Flame, Minus, Swords, Wheat } from 'lucide-react';
import { BackIcon } from './ui/icons';
import { ProfileIcon } from './ProfileIcon';
import { queueLabels, RoleStrip, splitRiotId, titleCase } from './Dashboard';
import { EmptyState, Notice, Skeleton } from './ui/States';
import { RankCrest } from '../runes/RankCrest';
import { ChampionIcon } from '../runes/ChampionIcon';
import { crestForTier } from './buildLogic';
import { MmrLine } from './MmrLine';
import { useChampionArt } from './art';
import { useAmbientChampion } from './AmbientArt';
import { fmtAgo } from '../runes/types';
import type { Game, Mmr, Snapshot } from './types';
import { profileGame, profileStats, type PlayerProfile, type PlayerLive } from './playerModel';
import { LiveGameContent } from './LiveGame';
import './PlayerPage.css';

const roleNames: Record<string, string> = { top: 'Top', jungle: 'Jungle', mid: 'Mid', adc: 'Bot', support: 'Support' };
const metrics = [['KDA', 2, '', Swords, '--ui-champ'], ['Kill participation', 0, '%', Crosshair, '--ui-win'], ['CS / min', 1, '', Wheat, '--ui-gold'], ['Damage / min', 0, '', Flame, '--ui-loss'], ['Vision / min', 2, '', Eye, '--ui-tier-s']] as const;
const metricIndexes = [0, 1, 2, 3, 5];
const rate = (wins: number, games: number) => games ? `${Math.round(100 * wins / games)}%` : '–';

function Trend({ points, average }: { points: number[]; average: number | null }) {
  const fill = useId();
  if (average == null || points.length < 2) return <span className="player-trend-empty">{points.length ? 'One observed game' : 'No observed data'}</span>;
  const min = Math.min(...points), max = Math.max(...points);
  const y = (v: number) => 30 - (max === min ? 12 : (v - min) / (max - min) * 24);
  const line = points.map((v, i) => `${4 + i * 152 / (points.length - 1)},${y(v)}`).join(' ');
  return <svg className="player-trend" viewBox="0 0 160 36" preserveAspectRatio="none" role="img" aria-label={`${points.length} games, oldest to newest; dashed line is their average`}>
    <defs><linearGradient id={fill} x1="0" y1="0" x2="0" y2="1"><stop stopColor="currentColor" stopOpacity=".24" /><stop offset="1" stopColor="currentColor" stopOpacity="0" /></linearGradient></defs>
    <polygon points={`4,36 ${line} 156,36`} fill={`url(#${fill})`} />
    <line x1="4" y1={y(average)} x2="156" y2={y(average)} className="player-trend-average" />
    <polyline points={line} />
    <circle cx="156" cy={y(points[points.length - 1])} r="2.8" fill="currentColor" />
  </svg>;
}

function RecordBar({ wins, games }: { wins: number; games: number }) {
  return <div className="player-record-bar" aria-hidden="true"><i style={{ width: `${games ? 100 * wins / games : 0}%` }} /></div>;
}

function Ranks({ profile, loading, mmr }: { profile: PlayerProfile | null; loading: boolean; mmr: Mmr | null }) {
  const rank = profile?.solo;
  const record = rank?.recordAvailable === true;
  return <section className="player-ranks panel" aria-labelledby="player-ranks-title">
    <div className="panel-head"><h2 id="player-ranks-title">Ranked standing</h2></div>
    <div className="player-rank">
        <RankCrest tier={rank ? crestForTier(rank.tier) : 'unranked'} size={88} mode="desktop" />
        <div><small>Solo/Duo</small>
          <strong>{rank ? `${titleCase(rank.tier)} ${rank.division}`.trim() : loading ? 'Loading…' : profile?.live && !profile.message ? 'Unranked' : 'Unavailable'}</strong>
          {rank && <p><b>{rank.lp} <small>LP</small></b></p>}
        </div>
    </div>
    {rank && <div className="player-rank-progress">
      {rank.division && <><div className="player-lp-caption"><span>Division progress</span><span>{rank.lp} / 100 LP</span></div><meter min={0} max={100} value={Math.min(100, rank.lp)} aria-label="LP toward next division" /></>}
      {record ? <><div className="player-rank-record"><span><b className="up">{rank.wins}W</b> · <b className="down">{rank.losses}L</b></span><b>{rate(rank.wins, rank.wins + rank.losses)} <small>win rate</small></b></div>{rank.recordSource === 'ugg' && <small className="player-record-source" title="Your League Client didn't return this player's season record.">via u.gg</small>}<RecordBar wins={rank.wins} games={rank.wins + rank.losses} /></>
        : <p className="player-record-unavailable">Season record unavailable</p>}
    </div>}
    {mmr && <div className="player-rank-mmr"><MmrLine mmr={mmr} explain={`Swapper's estimate, not Riot's: the average current Solo/Duo rank of the other players in this player's last ${mmr.games} ranked ${mmr.games === 1 ? "game" : "games"}.`} /></div>}
  </section>;
}

export default function PlayerPage({ riotId, snapshot, names, onBack, onOpenMatch, onOpenPlayer }: {
  riotId: string; snapshot: Snapshot | null | undefined; names: Record<string, string>; onBack: () => void;
  onOpenMatch: (game: Game) => void; onOpenPlayer: (riotId: string) => void;
}) {
  const [profile, setProfile] = useState<PlayerProfile | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');
  const [attempt, setAttempt] = useState(0);
  const [visible, setVisible] = useState(10);
  const [mmr, setMmr] = useState<Mmr | null>(null);
  const [live, setLive] = useState<PlayerLive | null>(null);
  const [showLive, setShowLive] = useState(false);
  const [liveLoading, setLiveLoading] = useState(false);
  useEffect(() => {
    let active = true;
    setLive(null); setShowLive(false); setLiveLoading(false);
    const check = () => {
      if (document.hidden) return;
      void invoke<PlayerLive>('full_app_player_live', { riotId, details: false })
        .then(next => { if (active) setLive(previous => ({ ...next, view: next.live ? previous?.view ?? null : null })); }).catch(() => {});
    };
    check();
    const timer = setInterval(check, 30_000);
    return () => { active = false; clearInterval(timer); };
  }, [riotId, attempt]);
  useEffect(() => {
    if (!showLive) return;
    let active = true;
    setLiveLoading(true);
    invoke<PlayerLive>('full_app_player_live', { riotId, details: true }).then(next => { if (active) setLive(next); })
      .catch(() => { if (active) setLive(previous => ({ live: previous?.live ?? false, view: null, message: 'Live players are unavailable. Try again shortly.' })); })
      .finally(() => { if (active) setLiveLoading(false); });
    return () => { active = false; };
  }, [showLive, riotId]);
  useEffect(() => {
    let active = true;
    setLoading(true); setProfile(null); setError(''); setMmr(null);
    invoke<PlayerProfile>('full_app_player_profile', { riotId }).then(next => {
      if (!active) return;
      setProfile(next);
      // Its rank lookups take a few seconds, so it follows the profile instead of delaying it.
      if (next.live) invoke<Mmr | null>('full_app_player_mmr', { riotId }).then(found => { if (active) setMmr(found); }).catch(() => {});
    })
      .catch(() => { if (active) setError('This profile is unavailable. Open League Client and try again.'); })
      .finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, [riotId, attempt]);
  const saved = useMemo(() => (snapshot?.games ?? []).map(game => profileGame(game, riotId)).filter((g): g is Game => g !== null), [snapshot, riotId]);
  const games = useMemo(() => [...new Map((profile?.games ?? saved).map(g => [g.id, g])).values()].sort((a, b) => b.createdAt - a.createdAt), [profile, saved]);
  const stats = useMemo(() => profileStats(games, profile?.profileIcons), [games, profile?.profileIcons]);
  const splash = useChampionArt(stats.champions[0]?.id, 'splash');
  useAmbientChampion('player', stats.champions[0]?.id);
  const secondSplash = useChampionArt(stats.champions[1]?.id, 'splash');
  const thirdSplash = useChampionArt(stats.champions[2]?.id, 'splash');
  const championArt = [splash, secondSplash, thirdSplash];
  const player = profile?.player ?? { riotId, icon: null, level: null };
  const [name, tag] = splitRiotId(player.riotId);
  const championName = (id: number) => names[id] ?? `Champion ${id}`;
  const knownRoles = stats.roles.reduce((total, role) => total + role.games, 0);
  if (showLive) return <div className="player-live-game">
    <button className="player-back" onClick={() => setShowLive(false)}><BackIcon size={16} />Back to {name}</button>
    <h2>{name}{tag} · Live game</h2>
    {liveLoading ? <Skeleton label="Loading live players" rows={10} height={54} />
      : live?.view ? <LiveGameContent view={live.view} error="" loaded championNames={new Map(Object.entries(names).map(([id, name]) => [Number(id), name]))} readOnly />
        : <EmptyState title={live?.message?.includes("aren't supported") ? 'Game mode not supported' : 'Live players unavailable'}>{live?.message ?? 'The game may have ended. Try again shortly.'}</EmptyState>}
  </div>;
  return <div className="player-page dash-v2">
    <button className="player-back" onClick={onBack}><BackIcon size={16} />Back</button>
    <div className="player-layout">
      <section className="player-hero ui-card is-raised" aria-label="Player profile">
        {splash && <img className="player-hero-art" src={splash} alt="" draggable={false} />}
        <div className="player-hero-content">
          <div className="player-hero-identity"><LevelFrame player={player} />
            <div className="player-identity"><h2>{name}{tag && <span>{tag}</span>}{live?.live && <button className="nav-badge live player-live-badge" aria-label="Open live game" onClick={() => setShowLive(true)}><i aria-hidden />LIVE</button>}</h2>
              {(!player.level || profile?.region) && <p>{player.level ? profile?.region : <>Level unavailable{profile?.region && <span> · {profile.region}</span>}</>}</p>}
              <small>{loading ? 'Loading profile…' : profile?.live ? 'From League Client · just now' : `Saved data${profile?.fetchedAt ? ` · ${fmtAgo(profile.fetchedAt)}` : ''}`}</small>
            </div>
          </div>
          <div className="player-hero-record"><div><strong>{rate(stats.wins, stats.games)}</strong><span>Recent win rate</span></div>
            <div><b className="up">{stats.wins}W</b><span> / </span><b className="down">{stats.games - stats.wins}L</b><small>Across {stats.games} loaded games</small></div>
            {stats.champions[0] && <p>Most played <strong>{championName(stats.champions[0].id)}</strong></p>}
          </div>
        </div>
      </section>
      <div className="player-notices">
        {!loading && !profile?.live && <Notice>Open League Client to load this profile. Saved games show what Swapper recorded.</Notice>}
        {profile?.message && <Notice action={<button disabled={loading} onClick={() => setAttempt(n => n + 1)}>Try again</button>}>{profile.message}</Notice>}
        {error && <Notice action={<button disabled={loading} onClick={() => setAttempt(n => n + 1)}>Try again</button>}>{error}</Notice>}
      </div>
      <section className="player-performance panel" aria-labelledby="player-performance-title">
        <div className="panel-head"><h2 id="player-performance-title">Performance</h2><small>{stats.games} loaded games · per-game averages</small></div>
        <div className="player-metrics">{metrics.map(([label, digits, unit, Icon, accent], i) => {
          const metric = stats.metrics[metricIndexes[i]];
          const delta = metric.delta == null ? null : Number(metric.delta.toFixed(digits));
          const trend = delta == null || delta === 0 ? 'flat' : delta > 0 ? 'up' : 'down';
          const Arrow = trend === 'up' ? ArrowUpRight : trend === 'down' ? ArrowDownRight : Minus;
          return <div className={`player-metric trend-${trend}`} key={label} style={{ '--metric-accent': `var(${accent})` } as CSSProperties}>
            <span className="player-metric-icon"><Icon size={19} aria-hidden="true" /></span>
            <span className="player-metric-label">{label}</span><strong>{metric.average == null ? '–' : metric.average.toFixed(digits)}<small>{metric.average == null ? '' : unit}</small></strong>
            <Trend points={metric.points} average={metric.average} />
            <span className="player-metric-delta" title={`Recent five loaded games versus the ${metric.samples}-game observed average`}>
              {delta != null ? <><Arrow size={16} aria-hidden="true" />{delta > 0 ? '+' : ''}{delta.toFixed(digits)}{unit} <small>vs avg</small></> : 'Unavailable in these games'}
            </span><small className="player-coverage">Last 5 · {metric.samples}/{stats.games} observed</small>
          </div>;
        })}</div>
      </section>
      <aside className="player-side">
        <Ranks profile={profile} loading={loading} mmr={mmr} />
        <section className="player-champions panel" aria-labelledby="player-champions-title">
          <div className="panel-head"><h2 id="player-champions-title">Top champions</h2><small>{stats.games} loaded games</small></div>
          {stats.champions.length ? <ol>{stats.champions.map((c, i) => <li key={c.id}>
            {championArt[i] && <img className="player-champion-art" src={championArt[i]!} alt="" loading="lazy" />}
            <div><strong>{championName(c.id)}</strong><small>{c.games} games · {c.kda.toFixed(2)} KDA</small></div>
            <div className="player-champion-record"><b className={c.wins >= c.losses ? 'up' : 'down'}>{rate(c.wins, c.games)}</b><small>{c.wins}W · {c.losses}L</small></div>
            <RecordBar wins={c.wins} games={c.games} />
          </li>)}</ol> : <p className="player-empty-copy">Champion performance appears after games load.</p>}
        </section>
        <section className="player-together panel" aria-labelledby="player-together-title">
          <div className="panel-head"><h2 id="player-together-title">Played with</h2><small>2+ games together</small></div>
          {stats.playedWith.length ? <ul>{stats.playedWith.map(p => <li key={p.riotId.toLowerCase()}><button onClick={() => onOpenPlayer(p.riotId)}>
            <ProfileIcon player={p} size={40} />
            <span className="player-together-name"><strong>{splitRiotId(p.riotId)[0]}</strong><small>{splitRiotId(p.riotId)[1]} · {p.games} games</small></span>
            <span className="player-together-record"><b className={p.wins * 2 >= p.games ? 'up' : 'down'}>{rate(p.wins, p.games)}</b><small>{p.wins}W · {p.games - p.wins}L</small><RecordBar wins={p.wins} games={p.games} /></span>
          </button></li>)}</ul> : <p className="player-empty-copy">No recurring teammates in these {stats.games} games.</p>}
        </section>
        <section className="player-roles panel" aria-labelledby="player-roles-title">
          <div className="panel-head"><h2 id="player-roles-title">Role split</h2><small>Estimated · {knownRoles}/{stats.games} games</small></div>
          <RoleStrip roles={stats.roles} />
        </section>
      </aside>
      {loading && !games.length ? <div className="player-games"><Skeleton label="Loading player games" rows={5} height={54} /></div> : <section className="panel player-games" aria-labelledby="player-games-title">
        <div className="panel-head"><h2 id="player-games-title">Recent games</h2><small>{Math.min(visible, games.length)} of {games.length}{profile?.live ? '' : ' · saved'}</small></div>
        {games.length ? <ol className="player-match-list">{games.slice(0, visible).map(game => {
          const opponent = game.opponent == null ? null : game.players[game.opponent];
          return <li className={`player-match match-row ${game.win ? 'is-win' : 'is-loss'}`} key={game.id}>
            <button className="player-match-open" onClick={() => onOpenMatch(game)} aria-label={`Open ${championName(game.championId)} ${game.win ? 'victory' : 'defeat'} match`}>
              <span className="match-champ"><ChampionIcon id={game.championId} name={championName(game.championId)} mode="desktop" /></span>
              <span className="player-match-result"><strong>{game.win ? 'Victory' : 'Defeat'}</strong><small>{queueLabels[game.queue] ?? game.queue} · {fmtAgo(game.createdAt)} · {Math.floor(game.duration / 60)}:{String(game.duration % 60).padStart(2, '0')}</small></span>
              <span className="player-match-kda"><strong>{game.kills} / {game.deaths} / {game.assists}</strong><small>{((game.kills + game.assists) / Math.max(1, game.deaths)).toFixed(2)} KDA</small></span>
              <span className="player-match-cs"><strong>{game.cs} CS</strong><small>{game.metrics[2]?.toFixed(1) ?? '–'} / min</small></span>
              <span className="player-match-opponent"><small>{opponent ? `vs ${championName(opponent.championId)}` : 'Opponent unknown'}</small><small>{roleNames[game.role] ?? 'Role unknown'} <span aria-hidden>›</span></small></span>
            </button>
          </li>;
        })}</ol> : <EmptyState title="No games available">Recent Rift games appear here when League Client can provide them.</EmptyState>}
        {visible < games.length && <button className="player-more" onClick={() => setVisible(n => n + 10)}>Show more <span>{games.length - visible} remaining</span></button>}
      </section>}
    </div>
  </div>;
}

/** The profile icon inside League's level frame (it changes every 25 levels), with the level on the frame's plate. */
function LevelFrame({ player }: { player: { riotId: string; icon: string | null; level: number | null } }) {
  const [border, setBorder] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    setBorder(null);
    if (player.level) void invoke<string>('level_border', { level: player.level }).then(url => { if (live) setBorder(url); }).catch(() => {});
    return () => { live = false; };
  }, [player.level]);
  return <span className={`player-level-frame${border ? ' has-border' : ''}`}>
    {border && <img className="player-level-border" src={border} alt="" draggable={false} />}
    <ProfileIcon player={player} size={82} />
    {player.level ? <b className="player-level-plate" title={`Level ${player.level}`}>{player.level}</b> : null}
  </span>;
}
