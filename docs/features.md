# Features

## Overview

Swapper is a free Windows app for League of Legends. It switches between saved Riot Client sign-ins without asking for your password, and in champion select it sets your runes, summoner spells and item build in one click. The **Fullsized App** adds a stats dashboard, match details with graphs and a kill map, a tier list with builds and matchups, a live draft helper and player search. An optional in-game overlay and a phone remote round it off.
Use it as a small tray flyout, as the Fullsized App, or both. The first launch asks which you prefer; **Settings → General → Run setup again** changes it later.

## Installing and updating

From v0.4 on, Swapper checks for updates itself and installs them with **Update & Restart**; older versions need v0.4 installed by hand once. Exit an older Swapper from the tray before installing it. Swapper stores its settings and encrypted sessions under `%LOCALAPPDATA%\Swapper`; reinstalling the app does not remove them.

## Fullsized App

- **Dashboard**: your rank and LP progress, today's games, wins and LP, your form over the last games, KDA, CS, damage and kill participation compared with players at your rank, your champions and roles, and your recent games, newest first, each with its lane opponent. Saved stats stay readable when League Client is closed.
- **Match details**: an overview with the facts that decided the game and a team comparison, a scoreboard with each player's rank, every loadout, a full stats table for all ten players, and graphs: the gold difference over time, both teams' objectives and kills on one time axis, and a kill map you can filter by player.
- **Tier list & Builds**: the strongest champions per role and rank, and for each champion its runes, spells, skill order, item build, pro builds and matchups. Apply runes, spells and the item set to the client in one click. Hover an ability to see its cooldowns, costs and range.
- **Matchups**: pick your champion and the enemy to see the win rate and the matchup build; your champion pool comes from your mastery.
- **Champion select (Draft)**: a live view of both teams and bans with pick suggestions, your matchup and the recommended build. The window comes to the front when champion select starts.
- **Location bar and search**: the title bar shows where you are (for example *Dashboard › Aatrox · Victory*); click a part to go back there. Click it or press `Ctrl+K` to search champions, pages, players and actions. A player's profile shows their Solo/Duo rank, their averages with trends, top champions, the players they queue with, their role split and recent games, read from your League Client. Click your own portrait in the sidebar to open your profile. When the client doesn't return a player's season record, Swapper fills it in from u.gg and labels it; if neither has it, the record is shown as unavailable rather than guessed.
- **Live game**: both teams with ranks while you are in a game.
- **Restart client** on the current account from the account menu, `Ctrl+K` or the tray icon's menu, for example when the client misbehaves. When League Client stops responding, the sidebar offers the restart. When League Client isn't open, the sidebar shows **Play**, which opens it on the current account. Swapper never restarts during a game and asks first in champion select, where a restart means a dodge. Turning **Launch through Deceive** on or off restarts the client the same way so the change applies.
- Pages show the champion, game or player you're looking at as a soft backdrop behind glass panels; numbers and tables sit on solid panels so they stay easy to read. Scrolling stays smooth on high-refresh screens.

## In-game overlay

- Optional overlay with the gold difference (hold a key to show it), objective timers and a skill level-up reminder. It reads League's local game data and draws a separate window; it does not inject into or read the game's memory. Needs Borderless or Windowed mode.

## Accounts

See [accounts.md](accounts.md) for the account features and for adding and switching accounts.

## Champion select

- Pick a recommended rune preset for your champion and role, copy a pro's page from the **Pro builds** tab, or edit every rune yourself in the **Editor** — from the tray flyout or the phone. The role follows champion select; the role buttons override it. A rank filter adjusts the [op.gg](https://op.gg) recommendation bracket.
- Presets can also set their summoner spells and, with **Import item build** on, add an item set to the League in-game shop: starter items, the full build, situational options grouped by purpose, and the ability max order in its title (for example `Swapper: Ahri · Max Q > W > E`). Swapper keeps only one of its own item sets, removes it after the game, and never touches yours.
- Once the enemy laner is revealed, a **vs {enemy}** option next to the generic presets offers a lane matchup build (runes, spells, items, skill order) with the matchup's win rate and sample size. Small samples fall back to the generic build, and auto-apply only uses a matchup with 500+ games.
- The **Champion** tab works before you pick: it shows a lane tier list (tier, win/pick/ban rate, games), and for any searched champion its tier, win rate, pick and ban rates, lane rank, damage split, a read-only common build, and its toughest and easiest matchups, all from lolalytics.
- Auto-applying the recommended preset when your champion locks in is a setting, off by default.
- Windows notifications when a ready check pops and when champion select starts, each with its own toggle.
- When op.gg, lolalytics or u.gg is slow or down, Swapper shows the last data it fetched, labelled with its age.

## Phone remote

- Use your phone over a trusted private Wi-Fi or Ethernet network, without Tailscale, to watch queue time, accept a match, prepick, ban, pick, and lock in a champion. Tailscale remains an optional transport. Champion search, role filters, and icons come from the running League client. The phone vibrates and chimes on a ready check while the page is open.
- Pair a phone by scanning a single-use QR code. Paired phones stay paired across restarts; rename or revoke them in **Settings → Remote**.

## Diagnostics

- **Connection checks** (Settings → About) checks Riot Client, League Client, the data providers, Remote Control and the installed version, and copies a sanitized report for bug reports.
