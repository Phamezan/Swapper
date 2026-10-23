import { createContext, useContext, useEffect, useState } from 'react';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { loadIcon } from '../runes/iconLoader';
import { useChampionArt } from './art';
import type { AmbientChampions } from './lookModel';

export const AmbientArtContext = createContext<(kind: keyof AmbientChampions, id: number | null) => void>(() => {});

/** Display-only page selection; no extra page requests or scroll subscriptions. */
export function useAmbientChampion(kind: keyof AmbientChampions, id: number | null | undefined) {
  const publish = useContext(AmbientArtContext);
  useEffect(() => {
    publish(kind, id || null);
    return () => publish(kind, null);
  }, [kind, id, publish]);
}

export function useAmbientArt(championId: number | null) {
  const splash = useChampionArt(championId, 'splash');
  const [neutral, setNeutral] = useState<string | null>(null);
  const needsNeutral = !splash;
  useEffect(() => {
    if (!needsNeutral || !isTauri()) return;
    let active = true;
    void loadIcon('ambient:client-v1', () => invoke<string>('ambient_background', { name: 'client' }))
      .then(src => { if (active) setNeutral(src); }).catch(() => {});
    return () => { active = false; };
  }, [needsNeutral]);
  return splash || neutral;
}

export function AmbientArt({ src }: { src: string | null }) {
  return <div className="ui-ambient" aria-hidden="true">
    {src && <img key={src} src={src} alt="" draggable={false} onError={event => { event.currentTarget.hidden = true; }} />}
  </div>;
}
