# Dashboard spec — Swapper Fullsized App (agreed with the owner)

Worktree `C:\Users\Pham2\.herdr\worktrees\Swapper\feat-full-app-ui`, branch `feat/full-app-ui`, HEAD `7dd6b0a`
(your redesign 3a5e5aa + the Live Game branch merged in). Follow `docs/visual-identity.md` and the full-app tokens you built.
All previous rules hold: do it yourself (no agents/subagents/sessions), no commits, no `cargo fmt` on the whole crate,
no new dependencies, isolated preview identity + temporary `LOCALAPPDATA`, performance first.

## Why
The owner dislikes the per-stat sparkline cards (DPM-style): they look bad and say little over ~13 games. DPM's dashboard is
"a bit of everything". Swapper's dashboard should be a compact **launchpad** on top and **one focused performance section**,
with Swapper's unique angle: it knows all of the user's accounts.

## Data window
Last 20 matches from the LCU (as today), filtered to Rift. No deeper history.

## Layout (top to bottom)
1. **Top strip** (one compact band):
   - Left: **active account** — profile icon over a small champion-splash wash (most-played champion), Riot ID, Solo/Duo rank
     crest + tier + LP + W–L, and client/game status (client closed / lobby / in queue / champ select). While a game is running
     this block becomes a **Live Game card** (queue, game time, your champion vs lane opponent, "Open Live Game").
   - Right: **other saved accounts** as compact cards: profile icon, name, Solo/Duo rank + LP, "last played 3d ago", record of
     its last games (e.g. 6W–4L), and a **Switch** button (reuse the existing switch flow and its confirmations/guards).
     Horizontally scrollable if many. Accounts never seen since this feature: "Sign in once to see stats".
   - Replace the separate big rank card and the full-width hero banner from the current dashboard.
2. **Performance section**:
   - Header: title, **queue filter** (All / Solo / Flex / Normal segmented control; default Solo/Duo when ≥5 Solo games in
     the 20, else All) and a **W/L strip**: 20 small squares oldest→newest (win/loss colour, remakes excluded), hover shows champion,
     KDA and date. This is the only chart on the page.
   - **Five stat tiles, no sparklines**: KDA, KP, CS/min, damage share, plus one **role stat** for the user's main role in the
     filtered games (support: vision/min; jungle: KP-adjusted or CS/min incl. camps — pick vision/min or gold/min if nothing
     better is available; laners: damage/min). Each tile: big value, and **"vs lane opponent"**: the opponent's average and the
     difference (e.g. `6.2` · `vs 6.9 (−0.7)`), coloured by whether the difference is good for the user (deaths: lower is good).
   - **Insight lines** (0–3) under the tiles, see rules below.
3. **Two columns at ≥ ~1400 px** (stack below that):
   - Left (wide): **Match history** rows (keep the current rich row: result, queue, time ago, duration, champion + level, spells,
     keystone, K/D/A + ratio, CS + CS/min, items, place chip) and add:
     a) **lane opponent** champion icon ("vs Zed") with your CS difference against them;
     b) **LP change** (`+18` / `−15`) when LP recording has data for that game;
     c) **click to expand** into all 10 players (champion, Riot ID, KDA, damage, CS, items), from data already fetched.
   - Right: **"Your champions & roles"**: top champions (portrait, games, W–L, win %, KDA) merged with the per-role win-rate strip.
- Drop: the six sparkline cards, the big rank card, the full-width hero. No recordings/collections/promo/score.

## Lane opponent (rules)
- Opponent = the enemy with the same estimated role (mid–mid, top–top, jungle–jungle, ADC–ADC, support–support), using the
  existing role estimate (Smite → jungle, support item → support, then lane/position).
- If the user's role or the opponent is ambiguous (two candidates, lane swap, missing data), **skip that game** for comparisons and
  show the sample, e.g. "vs lane opponent · 17 of 20 games".
- Pure function + unit tests (ambiguity, bot lane, remakes, missing participants).

## Insight rules
- Candidate patterns: win rate by deaths (≤5 vs >5); win rate when ahead vs behind your opponent in CS/min; best and worst
  champion (≥3 games); win rate by role; KP in wins vs losses; early game (first 10 min) **only if the LCU exposes timeline data**
  — verify `/lol-match-history/v1/game-timelines/{gameId}` (or the current equivalent) live; if absent or slow, skip it.
- Show a pattern only if **both groups have ≥4 games** and the win-rate gap is **≥20 percentage points** (or, for KP, ≥15 points).
  Rank by gap, show at most 3. If none qualify, show nothing (no filler).
- Plain factual wording with counts: "You win 78% (7/9) when you die 5 or fewer times, 27% (3/11) otherwise."
- Pure function + unit tests for thresholds and wording.

## LP recording (start now)
- After each game ends (existing gameflow watcher: `EndOfGame`/`PreEndOfGame` → then read ranked stats once), store a snapshot per
  account: timestamp, gameId if known, queue, tier, division, LP, wins, losses. Append to a small bounded file per account under
  `%LOCALAPPDATA%\Swapper` (e.g. keep the last 500), atomic writes, no polling. Also snapshot once at sign-in/startup if missing.
- Match a snapshot pair to a game to compute LP change; handle promotions/demotions (tier/division change) correctly; unit-test it.

## Per-account cache (for the account cards)
- Whenever an account is signed in, cache: Solo/Duo rank + LP, W–L, last-played time, record of its recent games. Store next to
  existing account data (follow `vault.rs` conventions; this is presentation data, not secrets). Read at dashboard open; no polling.

## States
- Client closed: last snapshot with its age (existing behaviour).
- <5 games in the filtered queue: hide insights; tiles show "n games".
- <4 comparable games vs opponents: tiles show the user's value only.
- No data at all (first launch): short empty state with the client status.

## Performance
- No new timers. All data from the existing snapshot + one LCU read per sign-in/game end. Expanded match rows render on demand.
- Measure Fullsized App open on Dashboard (release, whole tree, private bytes) before vs after; it should not grow (the sparkline
  cards and big hero go away).

## Done
`cargo test --manifest-path src-tauri/Cargo.toml --lib`, `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`,
`npm run build`, Node tests pass. Screenshots at 1440×900 and 1100×700 (Riot IDs masked), including one with several accounts and
one in a (fixture) live game. Write `HANDOFF-dashboard.md` and reply "dashboard done".
