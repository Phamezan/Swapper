import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
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
    let request = 0;
    const reload = () => {
      const version = ++request;
      void loadMatchupGuide(champion, enemy, lane)
        .then(value => { if (live && version === request) setResponse({ key, value }); }).catch(() => {})
        .finally(() => { if (live && version === request) setDone(key); });
    };
    const stop = listen("matchup_records_changed", reload).catch(() => () => {});
    reload();
    for (const championId of [champion, enemy]) void invoke<GuideAbilities[number]>("full_app_abilities", { championId })
      .then(value => { if (live) setAbilities(current => ({ ...current, [championId]: value })); }).catch(() => {});
    return () => { live = false; void stop.then(unlisten => unlisten()); };
  }, [champion, enemy, lane, key, active]);
  const result = active && response?.key === key ? response.value : null;
  const guide = result?.guide?.champion === champion && (result.guide.enemy === undefined || result.guide.enemy === enemy) && result.guide.lane === lane ? result.guide : null;
  return { guide, record: result?.record, abilities, loading: active && done !== key };
}
