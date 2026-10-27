import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { loadMatchupGuide } from "./loadMatchupGuide";
import type { MatchupGuideResult } from "./matchupGuide";
import type { GuideAbilities } from "./MatchupGuideContent";

export function useMatchupGuide(champion: number, enemy: number, lane: string) {
  const key = `${champion}-${enemy}-${lane}`;
  const [response, setResponse] = useState<{ key: string; value: MatchupGuideResult | null } | null>(null);
  const [done, setDone] = useState("");
  const [abilities, setAbilities] = useState<GuideAbilities>({});
  const active = champion > 0 && enemy > 0 && champion !== enemy;
  useEffect(() => {
    if (!active) return;
    let live = true;
    void loadMatchupGuide(champion, enemy, lane)
      .then(value => { if (live) setResponse({ key, value }); }).catch(() => {})
      .finally(() => { if (live) setDone(key); });
    for (const championId of [champion, enemy]) void invoke<GuideAbilities[number]>("full_app_abilities", { championId })
      .then(value => { if (live) setAbilities(current => ({ ...current, [championId]: value })); }).catch(() => {});
    return () => { live = false; };
  }, [champion, enemy, lane, key, active]);
  const result = active && response?.key === key ? response.value : null;
  const guide = result?.guide?.champion === champion && result.guide.enemy === enemy && result.guide.lane === lane ? result.guide : null;
  return { guide, record: result?.record, abilities, loading: active && done !== key };
}
