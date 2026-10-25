import { useEffect, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { cachedIcon, loadIcon } from "../runes/iconLoader";
import { SpellIcon } from "../runes/SpellIcon";
import { abilityDetails, perRank, type Abilities } from "./cooldownModel";
import "./cooldown-sheet.css";

/** A champion's passive and Q/W/E/R with base cooldowns per rank (no ability
 *  haste), and optionally their summoner spells. Loads only when rendered. */
export function CooldownSheet({ championId, spells, compact }: { championId: number; spells?: number[]; compact?: boolean }) {
  const [sheet, setSheet] = useState<Abilities | null>(null);
  const [failed, setFailed] = useState(false);
  const spellKey = spells?.join(",") ?? "";
  useEffect(() => {
    setSheet(null);
    setFailed(false);
    if (championId <= 0 || !isTauri()) return;
    let live = true;
    const spellIds = spellKey ? spellKey.split(",").map(Number) : undefined;
    void invoke<Abilities>("champion_abilities", { championId, spellIds })
      .then(value => { if (live) setSheet(value); })
      .catch(() => { if (live) setFailed(true); });
    return () => { live = false; };
  }, [championId, spellKey]);

  if (championId <= 0) return null;
  if (!sheet) return <section className="cd-sheet is-empty"><small>{failed ? "Abilities need League open." : "Loading abilities…"}</small></section>;
  return <section className={`cd-sheet${compact ? " is-compact" : ""}`} aria-label="Ability cooldowns">
    <header><h4>Cooldowns</h4><small>Base cooldowns, no ability haste</small></header>
    <ul>
      <li>
        <AbilityIcon path={sheet.passive.iconPath} letter="P" />
        <span><strong>{sheet.passive.name || "Passive"}</strong><small>Passive</small></span>
      </li>
      {sheet.spells.map(ability => {
        const cooldown = perRank(ability.cooldowns);
        const details = compact ? [] : abilityDetails(ability);
        return <li key={ability.key}>
          <AbilityIcon path={ability.iconPath} letter={ability.key} />
          <span>
            <strong>{ability.name}</strong>
            {details.length > 0 && <small>{details.join(" · ")}</small>}
          </span>
          <b className="cd-time">{cooldown ? `${cooldown} s` : "–"}</b>
        </li>;
      })}
    </ul>
    {sheet.summoners.length > 0 && <ul className="cd-summoners" aria-label="Summoner spells">
      {sheet.summoners.map(spell => <li key={spell.id}>
        <SpellIcon id={spell.id} mode="desktop" />
        <span><strong>{spell.name}</strong></span>
        <b className="cd-time">{spell.cooldown} s</b>
      </li>)}
    </ul>}
  </section>;
}

export function AbilityIcon({ path, letter }: { path: string | null; letter: string }) {
  const key = `ability:${path}`;
  const [src, setSrc] = useState<string | null>(() => (path ? cachedIcon(key) ?? null : null));
  useEffect(() => {
    if (!path || !isTauri()) { setSrc(null); return; }
    let live = true;
    void loadIcon(key, () => invoke<string>("ability_icon", { path }))
      .then(data => { if (live) setSrc(data); })
      .catch(() => {});
    return () => { live = false; };
  }, [key, path]);
  return <span className="cd-icon">
    {src ? <img src={src} alt="" draggable={false} /> : <i aria-hidden />}
    <kbd>{letter}</kbd>
  </span>;
}
