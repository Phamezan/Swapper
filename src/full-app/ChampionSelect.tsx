import { useEffect, useState, type ComponentProps } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Crosshair, LockKeyhole } from "lucide-react";
import type { RunesPanel } from "../runes/RunesPanel";
import type { KeystoneBuildView, ChampionOverviewView, MatchupView } from "../runes/types";
import { fmtPct, selectionFromPreset } from "../runes/types";
import { Switch } from "../components/ui/switch";
import { SpellIcon } from "../runes/SpellIcon";
import { BuildPanel } from "./BuildPanel";
import { ChampionArt } from "./MatchupStrip";
import { NearViewport } from "./NearViewport";
import { Picker, RankPicker, RolePicker } from "./Picker";

type Draft = { allies: { championId: number; position: string; local: boolean }[]; enemies: { championId: number; position: string; local: boolean }[]; bans: number[] };
export function ChampionSelect(props: ComponentProps<typeof RunesPanel>) {
  const view = props.view;
  const [draft, setDraft] = useState<Draft | null>(null);
  const [index, setIndex] = useState(0);
  const [items, setItems] = useState<KeystoneBuildView | null>(null);
  const [overview, setOverview] = useState<ChampionOverviewView | null>(null);
  const [matchup, setMatchup] = useState<MatchupView | null>(null);
  const [error, setError] = useState("");
  const preset = view?.presets[index] ?? view?.presets[0];
  useEffect(() => { setIndex(0); }, [view?.championId, view?.position, view?.tier]);
  useEffect(() => {
    let live = true;
    if (view?.phase === "ChampSelect") void invoke<Draft>("full_app_draft").then(value => { if (live) setDraft(value); }).catch(() => { if (live) setDraft(null); });
    else setDraft(null);
    return () => { live = false; };
  }, [view]);
  useEffect(() => {
    let live = true; setItems(null); setOverview(null); setMatchup(null); setError("");
    if (view?.championId) {
      if (preset) void props.onLoadBuild(view.championId, view.position, view.tier, preset.keystone, selectionFromPreset(preset)).then(value => { if (live) setItems(value); }).catch(reason => { if (live) setError(String(reason)); });
      void props.championApi.overview(view.championId, view.position, view.tier).then(value => { if (live) setOverview(value); }).catch(reason => { if (live) setError(String(reason)); });
      if (view.enemyChampionId) void props.onLoadMatchup(view.championId, view.enemyChampionId, view.position, view.tier).then(value => { if (live) setMatchup(value); }).catch(() => {});
    }
    return () => { live = false; };
  }, [view?.championId, view?.position, view?.tier, view?.enemyChampionId, preset?.keystone]);
  const active = view?.phase === "ChampSelect";
  const build = preset && items ? { runes: preset, spells: preset.spells, items, skillOrder: items.skillOrder ?? overview?.build?.skillOrder ?? null } : overview?.build ?? null;
  return <div className="fa-draft-page dash-scope">
    <header className="page-heading"><div><h1>Champion select</h1><p>{active ? "Your team, your matchup, your next move." : "Your draft companion. Ready when you are."}</p></div><span className="fa-status-pill">{active ? "LIVE DRAFT" : "WAITING FOR DRAFT"}</span></header>
    {props.error && <p className="fa-notice" role="alert">{props.error}</p>}{error && <p className="fa-notice" role="alert">{error}</p>}
    {!active ? <section className="fa-card fa-draft-empty"><div className="fa-draft-emblem"><Crosshair size={52} /></div><h2>{props.loading ? "Connecting to champion select…" : "Your next game starts here"}</h2><p>Enter champion select in League Client. Your team’s picks, revealed opponents and recommended build will appear automatically.</p><div className="fa-draft-placeholder-team">{["TOP", "JUNGLE", "MID", "BOT", "SUPPORT"].map(role => <span key={role}><Crosshair size={20} /><small>{role}</small></span>)}</div></section> : <>
      <section className="fa-draft-teams">{([['allies', 'Your team'], ['enemies', 'Opponents']] as const).map(([key, title]) => <div className="fa-card fa-team" key={key}><h3>{title}</h3><div className="fa-team-line">{(draft?.[key] ?? Array.from({ length: 5 }, () => ({ championId: 0, position: "", local: false }))).map((member, i) => <div className={`fa-team-member${member.local ? " is-you" : ""}`} key={i}><NearViewport className="fa-team-art">{member.championId ? <ChampionArt id={member.championId} name={`Champion ${member.championId}`} /> : <LockKeyhole size={24} />}</NearViewport><small>{member.local ? "YOU" : member.position || "Unrevealed"}</small></div>)}</div></div>)}</section>
      {!!draft?.bans.length && <div className="fa-draft-bans"><span>Banned</span>{draft.bans.map((id, i) => <NearViewport key={`${id}-${i}`}><ChampionArt id={id} name={`Banned champion ${id}`} /></NearViewport>)}</div>}
      <div className="fa-draft-controls"><h2>{view.championName || "Choose your champion"}</h2><RolePicker value={view.position} onChange={props.onPositionChange} /><RankPicker value={view.tier} onChange={props.onTierChange} /><label>Auto-apply<Switch aria-label="Auto-apply runes" checked={view.autoApply} disabled={props.busy} onCheckedChange={props.onToggleAutoApply} /></label></div>
      {matchup && <section className="fa-card fa-draft-matchup"><Crosshair size={24} /><div><h3>Lane matchup · {view.championName} vs {matchup.enemyName}</h3><p>{matchup.stats ? `${fmtPct(matchup.stats.winPct)} win rate across ${matchup.stats.games.toLocaleString()} games` : "Waiting for enough matchup games"}{matchup.fallback ? " · General build recommended" : ""}</p></div></section>}
      {view.championId > 0 && <>
        <div className="fa-draft-controls">{view.presets.length > 1 && <Picker label="Rune preset" value={String(index)} options={view.presets.map((preset, i) => ({ value: String(i), label: `${preset.title} · ${fmtPct(preset.winPct)}` }))} onChange={value => setIndex(Number(value))} />}
          {(["d", "f"] as const).map((slot, i) => <Picker key={slot} label={`Summoner ${slot.toUpperCase()}`} value={String(i ? view.spells.spell2Id : view.spells.spell1Id)} options={view.spells.available.map(spell => ({ value: String(spell.id), label: spell.name, icon: <SpellIcon id={spell.id} mode="desktop" /> }))} disabled={props.busy} onChange={value => props.onPickSpell(slot, Number(value))} />)}
        </div>
        <BuildPanel championId={view.championId} name={view.championName} lane={view.position} tier={view.tier} build={build} liveView={view} />
      </>}
      {props.shopNotice && <p className="fa-notice" role="status">{props.shopNotice.message}</p>}
    </>}
  </div>;
}
