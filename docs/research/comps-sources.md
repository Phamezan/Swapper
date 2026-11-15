# Comps catalog — research notes

Researched: 2026-11-14. Catalog: `src/full-app/compsData.ts` (types in `src/full-app/compsModel.ts`).
Nothing in the catalog is invented: every variation, answer and meta number below was counted from
real drafts. Analyst text (win conditions, tips, power curves, style assignment) is judgement grounded
in the champions, with no fabricated statistics.

## Window

- Tournaments (711 games joined across all three tables): LCK 2026 Rounds 3-4, LCK 2026 Season
  Play-In, LCK 2026 Season Playoffs, LPL 2026 Split 3, LPL 2026 Grand Finals, LEC 2026 Summer,
  LEC 2026 Summer Playoffs, LCS 2026 Summer, LCS 2026 Summer Playoffs, Worlds 2026 Play-In,
  Worlds 2026 Main Event. Dates in set: 2026-07-22 to 2026-11-09. Update (second pass, same day):
  Worlds 2026 Finals games 1-2 are now in PicksAndBansS7 (games 3-5 still empty, ScoreboardGames
  still has no Finals rows), so the join is 711 games; Finals games take patch 26.20 (the Worlds
  patch) and their winner from MatchScheduleGame.
- Patch range in scope: 26.14–26.20 (per-game `Patch` from ScoreboardGames: 26.14 ×111, 26.15 ×174,
  26.16 ×174, 26.17 ×154, 26.20 ×94, blank ×2). Worlds 2026 was played on patch 26.20.
- Pro meta numbers use the latest patch with enough games on its own: 26.20, 94 games
  (Worlds Play-In + Main Event). Presence = (picks + bans) / 94; pickPct = picks / 94;
  banPct = bans / 94. Each champion can be picked and banned at most once per game.

## Queries (Leaguepedia Cargo API)

Base: `https://lol.fandom.com/api.php?action=cargoquery&format=json` with a descriptive
User-Agent (`SwapperCompsResearch/1.0 ...`), `limit<=500`, ~10 s pause between requests, and a hard
stop on `ratelimited` errors. Table schemas were read from `Special:CargoTables/<table>`.

Scope filter used for all three game tables (`GameId LIKE` OR-chain):
`2026 Season World Championship%`, `LCK/2026 Season/Rounds 3-4%`, `LCK/2026 Season/Season Play-In%`,
`LCK/2026 Season/Season Playoffs%`, `LPL/2026 Season/Split 3%`, `LPL/2026 Season/Grand Finals%`,
`LEC/2026 Season/Summer Season%`, `LEC/2026 Season/Summer Playoffs%`, `LCS/2026 Season/Summer Season%`,
`LCS/2026 Season/Summer Playoffs%`.

1. `tables=ScoreboardGames&fields=GameId,Tournament,Team1,Team2,Winner,DateTime_UTC,Patch,VOD,N_GameInMatch,Team1Picks,Team2Picks` — 709 rows (500 + 209).
2. `tables=PicksAndBansS7&fields=GameId,MatchId,Team1,Team2,Winner,Team1Ban1..5,Team2Ban1..5,Team1Pick1..5,Team2Pick1..5,Team1Role1..5,Team2Role1..5,Team1PicksByRoleOrder,Team2PicksByRoleOrder` — 715 rows (500 + 215).
3. `tables=MatchScheduleGame&fields=GameId,MatchId,Blue,Red,Winner,Vod,N_GameInMatch` — 729 rows (500 + 229). Used for blue/red sides; its `Vod` was empty in scope, VODs came from ScoreboardGames `VOD`.
4. `tables=ScoreboardPlayers&fields=GameId,Champion,Role` with `GameId LIKE "2026 Season World Championship%"` — 940 rows (94 games × 10). Validation only: champion sets matched PicksAndBansS7 role-order comps in 94/94 Worlds games.

Joins and definitions:

- Joined on `GameId`: 709 games present in all three tables. `ScoreboardGames.Team1` is always the
  blue team (709/709 vs MatchScheduleGame `Blue`); PicksAndBansS7 `Team1` is series order, mapped to
  sides via MatchScheduleGame. Winner team names agree across tables in 709/709 games.
- Team comp per game: `TeamXPicksByRoleOrder` (Top, Jungle, Mid, Bot, Support), with ScoreboardGames
  `TeamXPicks` as fallback (needed once). Draft sequence rebuilt from per-team pick order plus the
  standard order B1, R1 R2, B2 B3, R3 R4, B4 B5, R5; bans likewise. VOD ids extracted from
  ScoreboardGames `VOD` YouTube URLs (`watch?v=` and `youtu.be/` forms; many games share a match VOD
  with different timestamps — see the timestamp fix below).
- VOD timestamps (2026-11-15 fix): re-queried ScoreboardGames `VOD` for the 709-game scope
  (Cargo, 100-row pages, pauses, generic User-Agent, stopped on `ratelimited` and resumed later).
  Each example in `compsData.ts` was matched uniquely by video id + teams + `N_GameInMatch`;
  the URL's `t=` value is stored as `vodStart` (seconds) next to `vod` and passed as `startSec`
  to the VOD player. 169 examples gained `vodStart`; 9 Game-1 examples whose Leaguepedia VOD
  URL has no `t=` had their `vod` dropped rather than guessing a start.
- Champion ids: Data Dragon `16.22.1` `champion.json` (`key` field), e.g. Naafiri 950, Yunara 804,
  Locke 805, Ambessa 799. `CHAMPION_IDS` now covers all 173 Data Dragon champions, so no used name
  falls back to 266 (verified programmatically; 266 occurs only for Aatrox).

Cross-check (source 2): gol.gg Worlds 2026 Main Event champion list
(`gol.gg/champion/list/season-S16/split-ALL/tournament-Worlds 2026 Main Event/`) lists Nocturne
16 picks / 46 bans as the top-presence champion, Renata Glasc 14/47, Cassiopeia 13/38. Our
Play-In + Main Event counts (94 games) are Nocturne 16/67 (88%), Renata Glasc 16/54 (74%),
Cassiopeia 17/44 (65%): same ordering and near-identical pick counts; ban and presence differ
because gol.gg's page is Main Event only and Leaguepedia's Finals scoreboard rows were incomplete
at query time (see Excluded). No browser-use or gol.gg scraping beyond this one page was needed.

## Variations (60) — how each was derived

Second-pass threshold (the 20-cap removed): a core recurs in at least 8 of the 711 games for at
least 3 different teams, and is either a bot-lane duo (bot + support picked together, joint rate
at least 1.2× the pair's independent rate) or a top+jungle, jungle+mid or top+mid pair picked
together at least 1.4× its independent rate in at least 12 games. The original 20 hand-written
variations are kept; 40 further cores meeting the threshold were added with the same construction:
non-core slot options are the most frequent real fillers in that slot inside the core's games
(core first, then frequency; max 3 options per slot). Draft tag per slot is the median global
draft position (1–10) of the slot's core (or top option) in those games: median ≤ 4 =
"Pick early", ≥ 8 = "Hold for last", otherwise "Flexible". Each variation carries 2–3 real
example games (Worlds first, then playoffs), with Win/Loss from the comp team's side and the
YouTube video id when Leaguepedia lists a VOD. Counts per style: Engage 23, Dive 13, Pick 11,
Poke 8, Protect the carry 5 (60 total). Engage leads because most recurring bot duos are engage
pairs; Protect the carry stays thin because its enchanter duos were already in the first 20.

| Variation (style) | Core | Games | Teams |
| --- | --- | ---: | ---: |
| Rumble + Jarvan IV (Engage) | Rumble top, Jarvan IV jungle | 38 | 21 |
| Orianna + Jarvan IV (Engage) | Orianna mid, Jarvan IV jungle | 25 | 18 |
| Orianna + Naafiri (Engage) | Orianna mid, Naafiri jungle | 23 | 17 |
| Xayah + Rakan (Engage) | Xayah bot, Rakan support | 51 | 24 |
| Ezreal + Karma (Poke) | Ezreal bot, Karma support | 61 | 30 |
| Caitlyn + Bard (Poke) | Caitlyn bot, Bard support | 43 | 26 |
| Jayce + Ziggs (Poke) | Jayce top, Ziggs bot | 18 | 16 |
| Yunara + Lulu (Protect the carry) | Yunara bot, Lulu support | 94 | 33 |
| Lucian + Milio (Protect the carry) | Lucian bot, Milio support | 128 | 38 |
| Zeri + Yuumi (Protect the carry) | Zeri bot, Yuumi support | 15 | 8 |
| Aphelios + Thresh (Protect the carry) | Aphelios bot, Thresh support | 10 | 8 |
| Ashe + Seraphine (Pick) | Ashe bot, Seraphine support | 86 | 32 |
| Ahri + Vi (Pick) | Ahri mid, Vi jungle | 21 | 14 |
| Varus + Nautilus (Pick) | Varus bot, Nautilus support | 19 | 11 |
| Jhin + Shen (Pick) | Jhin bot, Shen support | 25 | 15 |
| Rumble + Vi + Ahri (Dive) | Rumble top, Vi jungle, Ahri mid | 14 | 9 |
| Ambessa + Lee Sin (Dive) | Ambessa top, Lee Sin jungle | 24 | 20 |
| Galio + Naafiri (Dive) | Galio mid, Naafiri jungle | 13 | 9 |
| Kai'Sa + Nautilus (Dive) | Kai'Sa bot, Nautilus support | 15 | 15 |
| Akali + Lee Sin (Dive) | Akali mid, Lee Sin jungle | 10 | 8 |

Styles kept as the existing five (Engage, Poke, Protect the carry, Pick, Dive): the 2026 data fits
them, with Poke honestly thinner (3 variations) because long-range poke now lives mostly in bot
lanes (Ezreal + Karma, Caitlyn + Bard, Jayce + Ziggs) rather than mid-lane artillery.

Corrections to the old placeholder catalog, confirmed by the counts: "Varus poke" had Varus with
Karma/Ashe supports — real Varus (65 bot games) played with Nautilus ×19, Rell ×11, Alistar ×9,
Bard ×8, so Varus now appears as Varus + Nautilus (Pick). "Ashe + Karma" as a duo occurred once
(Team Liquid, LCS 2026 Summer Playoffs) versus Ashe + Seraphine ×86 for 32 teams, so Ashe + Karma
is gone and Ashe + Seraphine is in. Jinx (7 bot games), Kog'Maw (4) and Twisted Fate mid were too
rare in this window for variations and were dropped.

## Answers (37: 19 lane incl. 1 duo + 18 draft) — how each was counted

Lane (`kind: "lane"`): the threat was picked, and the answer is the opposing team's same-role
champion picked later in the same draft (lane-opponent counter-pick timing), counted over all
711 games. `games` is that exact count; `patch` is the latest patch where it occurred. The
bot-duo answer counts games where the answering duo's second pick was completed after the
threat duo's. Second pass added Rumble → K'Sante (23), Jarvan IV → Xin Zhao (22),
Lucian → Yunara (22) and Vi → Pantheon (20); counts for the original 15 were recounted on the
711-game join (Cassiopeia → Syndra 22, Lee Sin → Skarner 19, Milio → Rakan 18, duo 18).

Draft (`kind: "draft"`): the answer is a champion in a different role that the opposing team
picked after the threat was revealed. `lift = P(answer picked by opponent later | threat
revealed) / P(answer picked by a team in general)`, over the 711 games (1,422 team-games).
Thresholds: at least 10 such games, threat picked in at least 30 games, lift at least 1.5
(stored rounded to one decimal). 18 pairs pass: Ezreal → Twisted Fate (10, 1.8×),
Lee Sin → Twisted Fate (11, 2.2×), Orianna → Renekton (11, 1.6×), Lucian → Rakan (22, 2.8×),
Lucian → Lulu (23, 2.0×), Milio → Xayah (11, 2.0×), Seraphine → Sylas (10, 2.2×),
Shen → Corki (13, 2.1×), Camille → Skarner (10, 1.6×), Cassiopeia → Rell (11, 1.9×),
Cassiopeia → Shen (15, 1.7×), Nocturne → Renekton (13, 2.6×), Nocturne → Ahri (10, 1.7×),
Yunara → Renata Glasc (12, 2.2×), Yunara → Milio (15, 1.6×), Ashe → Milio (14, 1.6×),
Akali → Renekton (10, 2.4×), Locke → K'Sante (12, 1.5×).

Olaf → Jarvan IV checked explicitly with the draft method: Jarvan IV (enemy jungle) was picked
after an Olaf was revealed in 7 of 138 Olaf games (5.1%), against a 13.5% baseline pick rate —
lift 0.38, far below the 1.5 threshold. Jarvan IV is picked after Olaf *less* often than usual,
so it is not listed as an answer in either kind; the supported Olaf answers are the lane pick
Kled (18 games, lift 7.4×) — no cross-role Olaf answer passes (best cross-role: Trundle jungle,
7 games, 0.84×).

| Threat → Answer | Games | Latest patch |
| --- | ---: | --- |
| Rumble → Ambessa | 34 | 26.20 |
| Jayce → K'Sante | 27 | 26.20 |
| Olaf → Kled | 18 | 26.20 |
| Ryze → Syndra | 25 | 26.17 |
| Cassiopeia → Syndra | 23 | 26.20 |
| Syndra → Viktor | 14 | 26.20 |
| Jarvan IV → Lee Sin | 25 | 26.20 |
| Vi → Jarvan IV | 25 | 26.20 |
| Nocturne → Lee Sin | 11 | 26.20 |
| Lee Sin → Skarner | 18 | 26.20 |
| Ezreal → Jhin | 26 | 26.20 |
| Karma → Bard | 24 | 26.17 |
| Milio → Rakan | 19 | 26.20 |
| Renata Glasc → Neeko | 9 | 26.20 |
| Lucian + Milio → Xayah + Rakan | 19 | 26.20 |

Requested checks that the data does NOT support (left out on purpose):

- Olaf → Jarvan IV: Jarvan IV (enemy jungle) was picked after an Olaf top was shown in only 6 of
  137 Olaf games — Jarvan IV is simply the most-picked jungler overall (191 picks). The real
  same-role answer is Olaf → Kled (18 games). The placeholder's 98 games were invented.
- Kalista + Renata Glasc → Ashe + Seraphine: the two duos met in 6 games; Ashe + Seraphine was the
  later-completed (answering) duo in only 2 (the reverse happened 4 times). Not an answer. The
  supported bot-lane answer in the data is Lucian + Milio → Xayah + Rakan (19 games).

## Pro meta (patch 26.20, 94 games)

Top 10 by presence: Nocturne 88% (16 P / 67 B), Vi 76% (16/55), Renata Glasc 74% (16/54),
Cassiopeia 65% (17/44), Olaf 61% (18/39), Syndra 57% (23/31), Qiyana 57% (22/32), Ezreal 54%
(28/23), Lee Sin 53% (29/21), Jarvan IV 53% (25/25). Each entry's `answers` are the answer
champions from the Answers list for that threat; Qiyana has none because its best same-role
answer (Lee Sin) reached only 6 games, below the threshold used for the list.

## Excluded and why

- Worlds 2026 Finals games 3–5 (Gen.G vs Bilibili Gaming series): PicksAndBansS7 rows are still
  empty and ScoreboardGames still has no Finals rows, so those three games remain outside the
  711-game set. Finals games 1–2 (with picks in PicksAndBansS7) are included via the
  PicksAndBansS7 + MatchScheduleGame join, patch 26.20.
- One LCK Rounds 3-4 game is in PicksAndBansS7 but not ScoreboardGames; excluded with the Finals
  by the join (709, not 715).
- Locke (new mid, Data Dragon 805) appears in real comps (76 mid games) but is never a named core:
  his kit could not be verified from the sources used, so no mechanics are claimed for him. He
  still appears factually inside example-game detail strings. Same for Yunara (804) beyond
  "marksman carry" framing from her Data Dragon tags.
- Orianna → Locke was the most frequent Orianna answer by count (21 games) and was excluded for
  the same reason; Orianna answers in pro meta therefore show no entry from that pair.
- Champions with tiny samples in this window (e.g. Jinx 7 bot games, Kog'Maw 4, Azir 21 mid games)
  have no variations; pre-26.14 patches and non-major regions were out of scope by design.

## Refresh

Re-run the three scope queries above (respecting the pause and rate-limit stop), rebuild the
711-game join, recount cores (≥8 games, ≥3 teams; duos at ≥1.2× joint lift, other lane pairs at
≥1.4× and ≥12 games), same-role answers, cross-role draft answers (≥10 games, lift ≥1.5), and
the latest-patch presence table, then replace the affected entries in
`src/full-app/compsData.ts`. Verify with
`npx tsc --noEmit`, `node --test src/full-app/compsModel.test.mjs`, and
`node scripts/screenshots.mjs --only=comps`.
