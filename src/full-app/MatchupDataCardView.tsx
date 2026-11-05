import type { ChampionOption } from "../runes/types";
import { AbilityHover } from "./AbilityHover";
import { AbilityIcon } from "./CooldownSheet";
import { TIER_OPTIONS } from "../runes/types";
import { displayCardPatch, enemySpells, gameSentence, goldSentence, spellSentences, usableGold, wilson, type MatchupDataCard } from "./matchupDataCard";

export function DataCardLabel() { return <span className="mg-data-label">Data-based</span>; }

export function DataCardProvenance({ card }: { card: MatchupDataCard }) {
  const s = card.stats;
  const fetched = new Date(s.fetchedAt).toLocaleString("en-US");
  return <footer className="mg-data-provenance">
    <p>U.GG · Patch {displayCardPatch(s.patch)} · {card.lane === "adc" ? "ADC" : card.lane} · {TIER_OPTIONS.find(t => t.value === card.tier)?.label ?? card.tier} · World · Solo/Duo · {s.games.toLocaleString("en-US")} games</p>
    <p>{s.stale ? "Cached · fetched " : "Fetched "}<time dateTime={new Date(s.fetchedAt).toISOString()}>{fetched}</time></p>
    {card.enemyKit && <p>Enemy kit version {card.enemyKit.version} · fetched {new Date(card.enemyKit.fetchedAt).toLocaleString("en-US")}</p>}
  </footer>;
}

export function DataCardOverview({ card, champion, enemy }: { card: MatchupDataCard; champion: ChampionOption; enemy: ChampionOption }) {
  const { wins, games } = card.stats, interval = wilson(wins, games), gold = usableGold(card);
  return <div className="mg-data-overview">
    <div className="mg-data-heading"><strong>{champion.name} vs {enemy.name} · {card.lane === "adc" ? "ADC" : card.lane}</strong><DataCardLabel /></div>
    <p className="mg-data-subline">Game outcomes and enemy kit facts</p>
    <h3>Game win rate</h3>
    <p>{gameSentence(champion.name, enemy.name, card.lane, wins, games)}</p>
    <details className="mg-data-uncertainty"><summary>Sampling uncertainty · 95% interval</summary>
      <p>{(100 * interval[0]).toFixed(1)}%–{(100 * interval[1]).toFixed(1)}% (Wilson). This describes sampling uncertainty, not a prediction or a correction for selection bias.</p>
    </details>
    {gold && <p>{goldSentence(champion.name, enemy.name, gold.mean)}</p>}
    {(card.lane === "adc" || card.lane === "support") && <p>These game outcomes include different lane partners and opponents; they do not describe a fixed 2v2.</p>}
    <DataCardProvenance card={card} />
  </div>;
}

export function DataCardEnemyAbilities({ card, enemy }: { card: MatchupDataCard; enemy: ChampionOption }) {
  const kit = card.enemyKit;
  if (!kit) return null;
  return <div className="mg-data-abilities">
    <p>Base values exclude ability haste. Listed ability range is not guaranteed hit distance.</p>
    {enemySpells(card).map(spell => <section className="mg-data-spell" key={spell.key}>
      <AbilityHover championId={enemy.id} abilityKey={spell.key} sheet={kit.sheet}>
        <AbilityIcon path={spell.iconPath} letter={spell.key} /><strong>{spell.key} · {spell.name}</strong>
      </AbilityHover>
      {spellSentences(enemy.name, spell).map(line => <p key={line}>{line}</p>)}
    </section>)}
    <DataCardProvenance card={card} />
  </div>;
}
