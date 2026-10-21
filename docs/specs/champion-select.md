# Champion select spec — Fullsized App (agreed with the owner, 2026-10-21)

Follow `docs/visual-identity.md` and the full-app design tokens. Performance first; no new dependencies.
Reference for what to beat (not copy): DPM's champ select page — teams banner, builds/pro builds list, rune trees, skill grid,
items with "Imported" states, win probability and a comp radar.

## Decided
- **Two automatic modes** on one page:
  - **Planning** (before you lock in): pick suggestions large, build small. Compact draft band on top.
  - **Build** (after lock-in): build large, draft band small.
  The page switches by itself on lock-in, and back if you un-hover/swap before lock.
- **Draft band**: both teams' picks and hovers with champion art, your slot highlighted, small ban row, timer.
  **No** win probability, **no** comp radar, **no** composition flags, **no** middle widget.
- **Suggestions (planning mode)**: three tabs.
  - **Counters**: best picks into the revealed enemy laner (existing matchup data). Default when the enemy laner is known.
  - **Recommended**: good **blind picks** (see below). Default when the enemy laner is not known.
  - **Meta**: top tier-list picks for your role.
  - **Click = hover** the champion in the client (existing LCU pick/prepick plumbing used by the phone remote). A small
    **Lock in** button with a confirmation; never lock on a plain click.
- **Build mode**: build variant tabs (Most played, Highest win rate, "vs <enemy laner>" matchup build when the enemy is known,
  Pro builds), then runes (full trees, unchosen greyed), summoner spells, **skill order as a 1–18 grid** with the max order
  (Q › E › W) above it, and items (starter, boots, core path, situational).
- **Apply**: per-section status like DPM — Runes ✓ Applied / Spells ✓ / Item set ✓, each with its own Apply, plus **Apply all**
  and the existing auto-apply setting shown inline. Switching build tab updates the status immediately.
- **No ban suggestions.**
- **Lane cheat sheet** (build mode, once the enemy laner is known; also usable for any enemy):
  - Opponent's abilities with **cooldowns per rank** (and costs/ranges if cheap) from the LCU's local game data
    (`/lol-game-data/assets/...` champion JSON; verify the fields), their passive.
  - Their **usual summoner spells and starting items** (build data for that champion/role).
  - **Power spikes**: levels 2, 3 and 6, first completed item timing from the build.
  - Matchup win rate and games.
  - Written lane tips: **only** if a licensed source turns up later (no scraping). Not in v1.
- **Cooldown cheat sheet on the Live Game page too**: click/hover any enemy player → abilities with cooldowns per rank and
  their summoner spells. Same data and component. (Hold-key in-game overlay version: later.)

## Recommended blind picks
- Definition: champions that are good to pick without knowing the enemy laner — few hard counters, solid into most of the
  likely opponent pool, flex picks count.
- Show **several picks per role** (e.g. 5–8), **conservative**: a champion qualifies only with enough games, and safety weighs
  more than raw expected win rate. **No per-pick reason line** (owner's choice) — just the list (portrait, name, win rate,
  maybe a small "flex" tag if the champion is also played in another role).
- Score per candidate (all champions in the role above a minimum play rate):
  - expected win rate across likely opponents (each opponent's matchup win rate weighted by how often that opponent is played in
    the role and bracket, sample-shrunk);
  - safety: share of likely opponent games where the candidate is >~3 points below its average (hard counters) — heavy weight;
  - team comp v1: damage balance of your locked teammates (AD/AP from champion `tacticalInfo.damageType`), and bot-lane duo
    synergy from the existing duo data when your ADC/support is locked. Later: wider team synergy, enemy picks in other lanes,
    and a jungle "strong early laners" factor.
- **Bracket**: your current rank's bracket automatically (e.g. Diamond II → Diamond+), editable; follows account switches.
- **Data**: matchup tables per candidate (existing `runes::counters::load`, one request per champion, cached). **Background
  preload once per patch** for your 2 main roles and your bracket: spread out (one request every few seconds), only while no
  game is running, cached on disk for the patch; other roles on demand. Report measured network/CPU cost.
- **Jungle v1**: same score + damage balance.
- **Sanity list** (unit/fixture test against cached data; tune weights if they fail; not hard rules):
  good blind picks at Diamond+ should land in roughly the top third of their role:
  top Aatrox, Jayce · mid Ahri, Vladimir · ADC Ezreal, Tristana · support engage (Nautilus, Leona, Rell, Rakan).
  Bad: Nami. (Research: few hard counters + flex = good blind; setup-dependent or easily punished = bad.)

## Notes
- Live draft was never exercised end to end by previous workers; test with fixtures and in a real champ select.
- Names of other players are hidden in ranked champ select — no rank/form for them here (Live Game covers it after load).
