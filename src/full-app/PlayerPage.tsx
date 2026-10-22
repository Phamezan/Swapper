import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { BackIcon } from './ui/icons';
import { ProfileIcon } from './ProfileIcon';
import { MatchRow } from './Dashboard';
import { EmptyState, Notice, Skeleton } from './ui/States';
import { RankCrest } from '../runes/RankCrest';
import { crestForTier } from './buildLogic';
import { fmtAgo } from '../runes/types';
import type { Game, Snapshot } from './types';
import { profileGame, type PlayerProfile } from './playerModel';
import './PlayerPage.css';

export default function PlayerPage({ riotId, snapshot, names, onBack, onOpenMatch }: {
  riotId: string; snapshot: Snapshot | null | undefined; names: Record<string, string>; onBack: () => void; onOpenMatch: (game: Game) => void;
}) {
  const [profile, setProfile] = useState<PlayerProfile | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    let active = true;
    setLoading(true); setError('');
    invoke<PlayerProfile>('full_app_player_profile', { riotId }).then(next => { if (active) setProfile(next); })
      .catch(() => { if (active) setError('This profile is unavailable. Open League Client and try again.'); })
      .finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, [riotId, attempt]);
  const saved = (snapshot?.games ?? []).map(game => profileGame(game, riotId)).filter((g): g is Game => g !== null);
  const games = profile?.games ?? saved;
  const player = profile?.player ?? { riotId, icon: null, level: null };
  return <div className="player-page dash-v2">
    <button className="player-back" onClick={onBack}><BackIcon size={16} />Back</button>
    <section className="player-hero ui-card" aria-label="Player profile">
      <ProfileIcon player={player} size={72} />
      <div className="player-identity"><h2>{player.riotId}</h2><p>{player.level ? `Level ${player.level}` : 'Level unavailable'}</p>
        <small>{loading ? 'Loading profile…' : profile?.live ? 'From League Client · just now' : `Saved data${profile?.fetchedAt ? ` · ${fmtAgo(profile.fetchedAt)}` : ''}`}</small></div>
      <div className="player-ranks">{(['solo', 'flex'] as const).map(queue => {
        const rank = profile?.[queue];
        return <div key={queue}>{rank && <RankCrest tier={crestForTier(rank.tier)} size={46} mode="desktop" />}<span><small>{queue === 'solo' ? 'Solo/Duo' : 'Flex'}</small>
          <strong>{rank ? `${rank.tier.charAt(0)}${rank.tier.slice(1).toLowerCase()} ${rank.division}`.trim() : loading ? 'Loading…' : profile?.live && !profile.message ? 'Unranked' : 'Unavailable'}</strong>
          {rank && <small>{rank.lp} LP · {rank.wins}W {rank.losses}L</small>}</span></div>;
      })}</div>
    </section>
    {!loading && !profile?.live && <Notice>Open League Client to load this profile. Saved games show what Swapper recorded.</Notice>}
    {profile?.message && <Notice action={<button disabled={loading} onClick={() => setAttempt(n => n + 1)}>Try again</button>}>{profile.message}</Notice>}
    {error && <Notice action={<button onClick={() => setAttempt(n => n + 1)}>Try again</button>}>{error}</Notice>}
    {loading && !games.length ? <Skeleton label="Loading player games" rows={5} height={78} /> : <section className="panel player-games" aria-labelledby="player-games-title">
      <div className="panel-head"><h2 id="player-games-title">Recent games</h2><small>{games.length} {games.length === 1 ? 'game' : 'games'}{profile?.live ? '' : ' · saved'}</small></div>
      {games.length ? <ol className="match-list">{games.map(game => <MatchRow key={game.id} game={game} name={id => names[id] ?? `Champion ${id}`} isLast={false} onOpen={() => onOpenMatch(game)} />)}</ol>
        : <EmptyState title={loading ? 'Loading games' : 'No saved games'}>Recent Rift games appear here when League Client can provide them.</EmptyState>}
    </section>}
  </div>;
}
