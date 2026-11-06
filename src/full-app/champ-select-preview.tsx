import { TooltipProvider } from "./ui/Tooltip";
// Dev-only preview of the champion select page from fixtures, with a mocked
// Tauri IPC: `npm run dev`, then open /src/full-app/champ-select-preview.html
// (?fixture=planning|hover|locked|locked-blind|locked-guide; add &rune-fail to make Apply fail). Never part of the app build.
// Icons and the rune catalog come from Data Dragon; everything else is fixture
// data, and hover/lock/apply only change the fixture, never a League client.
import { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import type { RunesView } from "../runes/types";
import { ChampionSelect } from "./ChampionSelect";
import Champions, { type ChampionFocus } from "./Champions";
import { current, listeners } from "./champ-select-mock";
import "../runes/runes.css";
import "./dashboard.css";
import "./full-app.css";

function Preview() {
  const [view, setView] = useState<RunesView>(current());
  const [focus, setFocus] = useState<ChampionFocus | null>(null);
  useEffect(() => { const update = () => setView(current()); listeners.add(update); return () => { listeners.delete(update); }; }, []);
  const reload = async () => setView(await invoke<RunesView>("get_runes"));
  const championApi = {
    list: () => invoke("get_champion_list"), counters: () => Promise.reject(new Error("unused")), duoList: () => Promise.reject(new Error("unused")),
    overview: (championId: number, position: string | null, tier: string) => invoke("get_champion_overview", { championId, position, tier }),
    tierList: (position: string, tier: string) => invoke("get_tier_list", { position, tier }),
  } as never;
  return <div className="fa-frame" style={{ gridTemplateRows: "minmax(0, 1fr)" }}>
    <div className="dashboard-shell" style={{ gridTemplateColumns: "minmax(0, 1fr)" }}><main className="dashboard-main dash-scope">
      {focus ? <><button type="button" onClick={() => setFocus(null)}>← Back to draft</button>
        <Champions key={`${focus.champion?.id}-${focus.matchup?.enemy}-${focus.matchup?.lane}`} matchups focus={focus} onSelectionChange={() => {}} /></>
      : <ChampionSelect mode="desktop" view={view} loading={false} busy={false} error={null} shopNotice={null}
        onOpenMatchupGuide={(champion, enemy, lane) => setFocus({ champion, matchup: { enemy, lane } })}
        onApply={async () => {}} onToggleAutoApply={() => {}} onToggleSpellsWithRunes={() => {}} onToggleImportItems={() => {}}
        onPickSpell={(slot, spellId) => void invoke("apply_spell", { slot, spellId }).then(reload)} onPositionChange={() => {}} onImportItems={async () => {}}
        onLoadProBuilds={() => invoke("get_pro_builds")} onLoadBuild={() => invoke("get_keystone_build")}
        onLoadMatchup={(championId, enemyChampionId) => invoke("get_matchup", { championId, enemyChampionId })} championApi={championApi} onTierChange={() => {}} sessionKey="preview" />}
    </main></div>
  </div>;
}

createRoot(document.getElementById("root")!).render(<TooltipProvider><Preview /></TooltipProvider>);
