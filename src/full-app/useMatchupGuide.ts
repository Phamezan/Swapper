import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { loadMatchupGuide } from "./loadMatchupGuide";
import type { MatchupGuideResult } from "./matchupGuide";
import { selectGuideContent, dataCardEligible, STATS_MAX_AGE, type MatchupDataCard } from "./matchupDataCard";
import type { GuideAbilities } from "./MatchupGuideContent";

export function useMatchupGuide(champion: number, enemy: number, lane: string, tier = "emerald_plus", allowData = true) {
  const key = `${champion}-${enemy}-${lane}-${tier}-${allowData}`;
  const [response, setResponse] = useState<{ key: string; value: MatchupGuideResult | null } | null>(null);
  const [candidate, setCandidate] = useState<{ key: string; card: MatchupDataCard } | null>(null);
  const [done, setDone] = useState("");
  const [abilities, setAbilities] = useState<GuideAbilities>({});
  const active = champion > 0 && enemy > 0 && champion !== enemy && ["top", "jungle", "mid", "adc", "support"].includes(lane);
  useEffect(() => {
    if (!active) return;
    let live = true;
    let request = 0;
    let expiry: ReturnType<typeof setTimeout>;
    const reload = () => {
      clearTimeout(expiry);
      setCandidate(null);
      const version = ++request;
      void loadMatchupGuide(champion, enemy, lane)
        .then(async value => {
          if (!live || version !== request) return;
          setResponse({ key, value });
          if (!allowData || value?.lookup !== "absent" || !value.lookupExpiresAt || value.lookupExpiresAt <= Date.now()
            || value.guide?.enemy !== undefined) return;
          const card = await invoke<MatchupDataCard | null>("get_matchup_data_card", { championId: champion, enemyChampionId: enemy, position: lane, tier }).catch(() => null);
          if (!live || version !== request || Date.now() >= value.lookupExpiresAt) return;
          if (dataCardEligible(card, { championId: champion, enemyChampionId: enemy, lane, tier })) {
            setCandidate({ key, card });
            const until = Math.min(value.lookupExpiresAt, card.stats.fetchedAt + STATS_MAX_AGE);
            expiry = setTimeout(reload, Math.max(0, until - Date.now()));
          }
        }).catch(() => {})
        .finally(() => { if (live && version === request) setDone(key); });
    };
    const stop = listen("matchup_records_changed", reload).catch(() => () => {});
    reload();
    window.addEventListener("focus", reload);
    const timer = setInterval(reload, 6 * 3600e3);
    for (const championId of [champion, enemy]) void invoke<GuideAbilities[number]>("full_app_abilities", { championId })
      .then(value => { if (live) setAbilities(current => ({ ...current, [championId]: value })); }).catch(() => {});
    return () => { live = false; clearTimeout(expiry); clearInterval(timer); window.removeEventListener("focus", reload); void stop.then(unlisten => unlisten()); };
  }, [champion, enemy, lane, tier, allowData, key, active]);
  const result = active && response?.key === key ? response.value : null;
  const lookup = result?.lookup === "absent" && (!result.lookupExpiresAt || result.lookupExpiresAt <= Date.now()) ? "unknown" : result?.lookup ?? "unknown";
  const content = active ? selectGuideContent(result?.guide ?? null, lookup, candidate?.key === key ? candidate.card : null,
    { championId: champion, enemyChampionId: enemy, lane, tier }) : null;
  const guide = content?.kind === "authored" ? content.guide : null;
  return { guide, content, record: result?.record, abilities, loading: active && done !== key && !content };
}
