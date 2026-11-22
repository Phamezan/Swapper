# Comps rework: roster + Answers

Status: agreed 2026-11-16. Wireframes pending, then revise this spec, then build M1–M3.

## Problem

- **Your team** auto-builds from lobby → saved team → match-history suggestions, capped at 5, saved per signed-in account. A friend group of 8–14 with rotating subs fights it every session.
- **Answers** tab is a flat list of 37 hand-curated threat → answer cards with one sentence each. No grouping, no search, no depth, team link is buried text.

## Current code (starting point)

- UI: `src/full-app/Comps.tsx` (`Comps`, `Answers`, `PoolsDrawer`, `ProMeta`), `src/full-app/comps.css`.
- Model: `src/full-app/compsModel.ts` (`selectTeamMembers`, `teamMode`, `coverageFor`, `applyOverrides`), tests in `compsModel.test.mjs`.
- Data: `src/full-app/compsData.ts` (`ANSWERS`, `Answer` type, `CURRENT_PATCH`), tests in `compsData.test.mjs`.
- Backend: `src-tauri/src/full_app/comps.rs` — commands `comps_team`, `comps_pools`, `comps_save_team`, `comps_save_overrides`, `comps_resolve` (registered in `src-tauri/src/lib.rs`). Store `comps-by-account.json` under `vault::data_root()`, keyed by signed-in PUUID. `season_suggestions` builds history suggestions.
- Reusable: matchup guides (`MatchupGuide*.tsx`), in-app VOD player (`VodPlayer.tsx`, `MatchupVodEntry.tsx`), Lolalytics client (`src-tauri/src/runes/lolalytics/`), `ChampionIcon`, `ChampionArt`, `ui/States`, `ui/PageHeader`.

## 1. Roster

- One roster (8–14 people typical, no hard cap below 20) **shared across all Swapper accounts**. "You" = whichever account is signed in; never stored as a roster member.
- Members keyed by PUUID. Fields: `puuid`, `riotId` (refreshed when resolvable), optional `nickname`, ordered `roles` (1–5 of top/jungle/mid/bot/support).
- Display: nickname if set, else Riot ID game name. Full Riot ID on hover.
- Add paths:
  1. Riot ID box (`Name#TAG`, resolved via `comps_resolve`).
  2. From current lobby (see §2).
  3. **Suggested** list from match history (`season_suggestions`) — one-click add. History never adds anyone automatically.
- Preferred roles pre-filled from the member's recent games when first added; editable (reorder, add, remove).
- Remove member, edit nickname, edit roles from the roster panel.
- No migration from `comps-by-account.json` saved teams (feature unused). Per-account `overrides` (added/hidden champions) stay as they are.
- No roster sharing/export.

## 2. Active players

- Session state: which roster members are active, max 5 including You.
- **You** active by default; toggleable off (planning for friends).
- **Lobby merge**: when in a lobby, roster members present in the lobby become active automatically. Lobby players not in the roster appear as an add chip (wording TBD after wireframe review — not "non-roster player"). Manual toggles still work and win over auto-select until the lobby changes.
- **Role auto-assign**: active players get roles from their ordered preferences with no clashes (maximize preference rank). Manual per-session override per player; a clash the solver can't resolve is shown, not silently broken.
- Active set + overrides persist for the session (in memory + last-used on disk is fine); roster persists on disk.

### Team bar UI

- Bar shows only active players: avatar/champ art, display name, role badge.
- **Roster** button opens a panel: every member as a toggle chip (role badges, edit affordance), Suggested list, Riot ID add box, lobby add chips.
- Empty roster state: bar invites adding the first friend.

## 3. Comps tab

- **Tailored only when exactly 5 are active.** Otherwise the default untailored view. No hint text about activating 5.
- Card design unchanged this project.
- With 5 active: a comp the five can't fully play shows a **single-swap sub hint** from inactive roster members ("Swap Tsunade → Insidious to play this"). Comps needing 2+ swaps hidden under the "Our pools" filter.
- Pools load for the whole roster (not just active) to power sub hints; existing 6 h pool cache (`POOL_CACHE_TTL`) applies.
- Pro meta tab untouched.

## 4. Answers tab

- **Threat-first.** Tabs: Top / Jungle / Mid / Bot / Support / Draft. Search box filters threats and answers by champion name.
- One row per threat champion (per role), listing all its answers ranked (pro first, then SoloQ).
- Draft tab: two-champion threats / draft answers (picked-after lift), separate from lane counters.
- **Tailoring**: any number of active players (1–5). "Your team can" shows active players who play the answer; sub hints from the rest of the roster. No active players = default untailored view. Filter: Our pools / All.
- **Expanded row**:
  - a) Stats: games, win rate of answer into threat, patch trend.
  - b) Pro games list: teams, result, link to Pro meta games.
  - c) VOD jump into the in-app player at the matchup (when VOD exists).
  - d) Link to the full matchup guide (answer vs threat) when one exists.
  - e) Team strip: active + roster players who play the answer, mastery, recent games.
  - f) Longer "why it works" + 2–3 short tips (follow guide rule: tips self-contained, no "the chart"/"the source" references).
- **SoloQ rows**: clearly labelled "SoloQ", only where no pro answer exists for that threat + role.
- Older-meta entries tagged "older meta".
- Empty search: clear no-results state.
- No champ select tie-in this project.

## 5. Answers data

- **Bundled JSON dataset**, rebuilt each patch by a sync script (sibling of `scripts/syncGuides`).
  - Backbone: Leaguepedia Cargo (PicksAndBans, ScoreboardGames, MatchScheduleGame) — picks, roles, win/loss per game.
  - Cross-check: gol.gg.
  - dpm.lol and other sites: research references for the prose pass only — no scraping.
- **Prose pass**: Muse Wonder agents write "why it works" + tips per entry.
- **SoloQ**: live from Lolalytics at runtime via existing client.
- Thresholds (one config block):
  - Pro lane counter: answer directly opposite threat, ≥ 8 games, ≥ 55% win rate.
  - Pro draft answer: picked after threat ≥ 1.5× its base rate, ≥ 8 games, ≥ 50% win rate.
  - Entries under 8 games are hidden, not tagged.
  - UI shows top 3 answers per threat row, rest behind "+N more".
  - SoloQ: win-rate delta ≥ +2% over ≥ 1,000 games; only when no pro answer exists.
  - Window: last 4 patches; older = "older meta".
- **First M3 step**: confirm Cargo exposes lane-opponent pairing (ScoreboardGames role per side). If not, report before building.
- Schema (M2 fixes it; draft): `{ id, kind: "lane"|"draft", role, threat: string[], answer: string[], source: "pro"|"soloq", patch, games, winRate?, lift?, trend?, why, tips: string[], proGames?: {teams, result, gameId, vod?}[], sources: string[] }`.

## 6. Deliverables and milestones

1. Wireframes (Fable 5.2 agent): high-fidelity clickable HTML in `docs/wireframes/comps/` numbered `05-…` onward, reusing `shell.css` / `common.js`, real app dark theme + `comps.css` tokens, CommunityDragon champion art. States:
   1. Team bar: empty roster, roster panel open (suggestions + Riot ID add), 3 active, 5 active.
   2. Lobby detected with add chip.
   3. Role clash with manual override.
   4. Answers: default untailored, tailored, expanded row, Draft tab, SoloQ row, search no results.
   5. Comps card with a single-swap sub hint.
2. User reviews wireframes, spec revised.
3. Muse Wonder agents:
   - **M1 Roster**: backend store (global roster file), lobby merge, role auto-assign, team bar + roster panel UI, Comps tailoring + sub hints.
   - **M2 Answers UI** on the current 37 entries reshaped to the new schema.
   - **M3 Data pipeline**: sync script, prose pass, SoloQ rows.
   - M1 and M2 in parallel; M3 after M2 fixes the schema.

## 7. Design decisions (wireframe review, 2026-11-16)

Wireframes are the visual source of truth: `docs/wireframes/comps/05-index.html` (06 team bar, 07 lobby, 08 role clash, 09 Answers, 10 Comps sub hint). Mock data only; wireframes link live `comps.css`.

Team bar:
- Tight row of round avatars = League **profile icons** (`https://cdn.communitydragon.org/latest/profile-icon/{id}`, `profileIconId` already returned by `comps_pools`), never champion icons.
- Small client position icon on each avatar's bottom edge (`https://raw.communitydragon.org/latest/plugins/rcp-fe-lol-clash/global/default/assets/images/position-selector/positions/icon-position-{top,jungle,middle,bottom,utility}.png`). Click opens role menu: every role with open/taken-by + "off-role"/"1st choice" labels, "Back to auto-assign", "Sit out this session". Manual role = lime ring.
- Small visible name under each avatar (ellipsis); hover shows full Riot ID + role.
- Only active players shown; no empty placeholder slots. Zero active: "Nobody active".
- Role clash: no banner. Amber ring + small "!" dot per clashing player; hover: "<A> and <B> both want <Role>. Nobody is on <Role>. Click the role icon to change it." Fix via role menu only.
- Lobby: lobby pill; blue dot on lobby members. Lobby player not in roster = subtle ghost avatar at the end of the row (see 07), hover "New teammate", one click adds to roster **and** makes active.
- Roster panel opens inline inside the team bar card (not a drawer). Panel lobby heading: "New teammates in your lobby".

Behaviour:
- Sub hints never swap You out. Sub hint on a Comps card is clickable and performs the swap.
- Comps "Our pools" filter (rename from "Playable by us") = 5/5 comps + one-swap comps.
- Answers "Our pools" keeps an answer if anyone on the roster (active or sub) plays it. Ranks fixed, so a row may show only e.g. rank 3.
- Pool matching ignores role: "Your team can" and comp coverage check champion pools only.
- Search filters the current tab; other tabs show match counts; empty state links to tabs that match.
- SoloQ expanded row: pro games column replaced by a "no pro answer" note.
- CommunityDragon champion art by numeric id / alias (Wukong = `MonkeyKing`), never display name.

Data (final):
- Dataset: `docs/research/answers/answers-numbers.json` (133 entries after thresholds) + prose `answers-<role>.json` (`{id, why, tips, sources, lowConfidence?}`, keyed by id; extra ids for dropped entries are ignored). Built by `docs/research/answers/build-answers.mjs`.

Round 2 (2026-11-16), wireframes 11–14:

- Roster cards show League position icons in preference order (main role full strength, rest dimmed) instead of TOP/JNG/MID/BOT/SUP text badges.
- The "Add by Riot ID" box is a dashed [+] tile at the end of the roster; clicking it turns it into the Riot ID input in place, and a successful add opens the member editor.
- Member edit card: position-icon role buttons with numbered order badges, a read-back sentence ("Bot first, then Mid."), "Remove from roster" as a quiet link, and Done as the only button.
- The Answers tab is renamed Counters in all user-facing copy (internal identifiers stay).
- Expanded counter row is option A of wireframe 12: one stat line instead of stat boxes, plain numbered tips, the VOD leading the right column, and roster chips + matchup guide link in a footer; there is no separate "Who plays" column.
- Filters are option A of wireframe 13 on both Comps and Counters: nothing is filtered by default, playable items are ringed and sort first with the rest dimmed below a divider, and one switch hides the rest. An emptied column or list explains why with a "+N hidden · show" affordance. There is no silent auto-switch to a pool filter when solo.
- "Playable" means: with fewer than 5 active, a comp that uses a champion an active player plays; with 5 active, 5/5 or one-swap comps (the former "Our pools" semantics); on Counters, anyone on the roster plays the counter.
- Solo Comps cards drop the role badge; the "You play X" line and the lime ring on your pick carry the signal.
- Counter tiles show only roster profile icons (active lime ring, sub gold faded, name on hover), no "Your team can" text.

## Out of scope

Comps card redesign, Pro meta redesign, champ select tie-in, roster sharing, saved-team migration.
