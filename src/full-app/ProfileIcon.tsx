import { useEffect, useState } from 'react';
import { AccountsIcon } from './ui/icons';
import type { PlayerHit } from './playerModel';

export function ProfileIcon({ player, size = 32 }: { player: Pick<PlayerHit, 'riotId' | 'icon'>; size?: number }) {
  const [failed, setFailed] = useState(false);
  useEffect(() => setFailed(false), [player.icon]);
  return player.icon && !failed ? <img className="search-profile-icon" src={player.icon} width={size} height={size} alt="" onError={() => setFailed(true)} />
    : <span className="search-profile-icon is-fallback" style={{ width: size, height: size }} aria-label="Profile icon unavailable"><AccountsIcon size={Math.round(size * .55)} aria-hidden /></span>;
}
