# Swapper TODO — Fullsized App, overlay, live game

State on 2026-10-21. All work is on **local branches only** (nothing pushed, nothing merged to `main`).
`feat/full-app-ui` contains everything so far:

| Branch | Content |
|---|---|
| `feat/ingame-overlay` | Native GDI+ in-game overlay: gold diff, objective timers, skill level-up (opt-in) |
| `feat/dashboard-window` | First optional dashboard window (superseded by the full app) |
| `feat/full-app` | Fullsized App replaces the flyout, low memory mode, first redesign |
| `feat/live-game` | Live Game page (ranks/form for all 10 players, LCU + Live Client only) |
| `feat/full-app-ui` | Graphite + green identity, full-app-native pages, styled pickers, matchups strip; **merges all of the above** |

Specs: `docs/visual-identity.md`, `docs/specs/dashboard.md`, `docs/specs/champion-select.md`.
Worker handoffs (untracked, in each worktree under `C:\Users\Pham2\.herdr\worktrees\Swapper\`): `feat-ingame-overlay\HANDOFF.md`,
`feat-full-app\HANDOFF-phase1.md` + `HANDOFF-phase2.md`, `feat-live-game\HANDOFF-live-game.md`, `feat-full-app-ui\HANDOFF-ui.md`.

## 1. Build (designed, not started)
- [x] **Dashboard redesign** (other-account cards and the in-game card not yet seen with real data) — `docs/specs/dashboard.md`
  - [x] Top strip: active account (rank, status; becomes a Live Game card in game) + other accounts (rank, last played, record, Switch)
  - [x] Performance: queue filter (default Solo/Duo if ≥5 games), 20-game W/L strip (only chart)
  - [x] 5 tiles: KDA, CS/game (support: vision/min), damage/game vs same champion + tier (lolalytics `build-full` sidebar averages), KP and damage share vs lane opponent
  - [x] Up to 3 insight lines (≥4 games per group, ≥20 pt gap, plain wording)
  - [x] Match rows: + lane opponent icon with CS diff, + LP change, + click to expand all 10 players
  - [x] "Your champions & roles" column; drop sparkline cards, big rank card, full-width hero
  - [x] LP recording after each game, per account (Solo/Duo; `full_app/lp.rs`, `lp-history.json`) + LP chip on match rows
  - [x] Per-account rank/record cache for the account cards (`full_app/cards.rs`, `account-cards.json`, `cards` in the dashboard view)
  - [ ] Empty/thin-data states
- [ ] **Champion select redesign** — `docs/specs/champion-select.md`
  - [ ] Planning mode / build mode, auto-switch on lock-in; compact draft band (no win %, no radar)
  - [ ] Tabs: Counters / Recommended / Meta; click = hover in client, Lock in with confirmation
  - [ ] Build tabs (most played, highest WR, vs enemy laner, pro builds), full rune trees, 1–18 skill grid, items
  - [ ] Per-section apply status (Runes/Spells/Item set) + Apply all + inline auto-apply
  - [ ] Lane cheat sheet: enemy cooldowns per rank, their summoners + starters, power spikes, matchup WR
- [ ] **Recommended blind picks** (part of champ select spec)
  - [ ] Conservative score: expected WR across likely opponents + heavy safety (few hard counters), sample-shrunk
  - [ ] Bracket = your rank automatically; all champions in role; 5–8 picks; no reason line
  - [ ] Team comp v1: damage balance + bot duo synergy
  - [ ] Background preload once per patch (2 main roles, your bracket, trickled, never in game); measure cost
  - [ ] Sanity test list (Aatrox, Jayce, Ahri, Vladimir, Ezreal, Tristana, Nautilus/Leona/Rell/Rakan good; Nami bad)
- [ ] **Cooldown cheat sheet on the Live Game page** (click/hover an enemy) — same component as champ select

## 2. Verify in a real game (never tested live)
- [ ] **Overlay gold estimate**: check whether Live Client `items[].price` is the full item cost or only the combine cost.
      If it's the combine cost, switch to full item cost (Data Dragon / local item data) — otherwise gold is badly underestimated.
- [ ] Overlay: drawing over the game in Borderless, alt-tab/minimize, game end cleanup, FPS impact, Tab hold behaviour
- [ ] Live Game: do enemy players get a puuid in the in-game `gameflow/session`? (If not, enemy rows have no rank.) Runes for all 10?
- [ ] Champion select live draft end to end (hover/pick/lock events, auto-switch to the page, apply)
- [ ] Live Game auto-open setting: window opens minimized without stealing focus from a fullscreen/borderless game

## 3. Review and ship
- [ ] Fresh reviewer session (different context) on `feat/full-app-ui` vs `main` before merging
- [ ] Merge into `main` — another session works on `main` ("Winrate build tabs"); expect conflicts in `App.tsx` / `src/runes/*`
- [ ] Release notes + version bump (see `docs/RELEASING.md`); README feature list (Fullsized App, overlay, Live Game)

## 4. Later / ideas (not designed yet)
- [ ] Player search by Riot ID via LCU (`/player-account/aliases/v1/lookup`, ranked + match history by puuid); Ctrl+K summoner search
- [ ] Key-backed proxy (e.g. Cloudflare Worker) for search when the client is closed / other regions / others' live games — never ship a key
- [ ] In-game overlay widgets: hold-key live game ranks; enemy cooldowns
- [ ] Overlay: Herald/Grubs timers; exclusive-fullscreen detection hint
- [ ] Blind picks: wider team synergy, enemy picks in other lanes, jungle "strong early laners" factor
- [ ] Written lane tips if a licensed source turns up (no scraping)
- [ ] Lower full-app icon memory: serve icons via a custom URI scheme instead of base64 data URLs (touches shared flyout icon components + CSP)
- [ ] LP graph per account once enough LP history is recorded

## 5. Cleanup (local machine)
- [ ] Stop leftover preview/dev processes: full-app release preview (PID 270200 at time of writing), codex worker's Vite on port 1437 (PID 307816)
- [ ] Delete preview data folders: `%LOCALAPPDATA%\com.phamezan.swapper.fullapppreview`, `...livepreview`, `...uipreview`, and the scratch preview `LOCALAPPDATA` folders
- [ ] Close idle herdr workspaces (`fullui2` w5D, `livegame` w5E) and remove merged worktrees after merging to `main`

## Decided (for reference)
- Low memory mode stays **off** by default (flyout reopen 0.42 s vs 0.02 s; ~100 MB idle is fine vs DPM's ~1 GB).
- Live Game auto-open setting **off** by default (~142 MB during a game).
- No DLL injection into League (Vanguard); overlay works in Borderless/Windowed only.
- Codex quota fits one codex worker; extra workers are Claude Opus 5.5.
