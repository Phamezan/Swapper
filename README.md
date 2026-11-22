# Swapper

Swapper is a free Windows app for League of Legends. It switches between saved Riot Client sign-ins without asking for your password, and in champion select it sets your runes, summoner spells and item build in one click. The **Fullsized App** adds a stats dashboard, match details, a tier list with builds and matchups, a live draft helper and player search.

## Demo

https://github.com/user-attachments/assets/3b516b9b-e45b-488c-b0ab-df282f6b52c0

A 21-second tour (also as an [MP4 file](docs/media/swapper-v2.mp4)): the account switcher, champion select, the Dashboard, match graphs, the tier list and the phone remote. All accounts and players are samples.

## Download

Download the Windows installer from the [latest GitHub release](https://github.com/Phamezan/Swapper/releases/latest). Swapper checks for updates itself and installs them with **Update & Restart**. The installer is currently unsigned, so Windows may ask you to confirm that you trust it.

Use it as a small tray flyout, as the Fullsized App, or both. The first launch asks which you prefer; **Settings → General → Run setup again** changes it later.

## Features

Details are in [docs/features.md](docs/features.md).

**Fullsized App**

- Dashboard with rank, LP, form, KDA and recent games; saved stats stay readable when League Client is closed.
- Match details with scoreboard, gold and objective graphs, and a kill map.
- Tier list, champion builds and matchups, with one-click apply of runes, spells and items.
- Live draft helper, live game view, and `Ctrl+K` search for champions, pages and players.

**Champion select**

- Recommended, pro and custom rune pages, summoner spells and an in-shop item set, with lane matchup builds.
- Optional auto-apply, ready-check and champion-select notifications.

**Accounts**

- Save multiple Riot Client sign-ins and switch from the tray flyout, desktop shortcuts or a hotkey; see [docs/accounts.md](docs/accounts.md).

**In-game overlay**

- Optional gold difference, objective timers and skill level-up reminder; it does not inject into or read the game's memory.

**Phone remote**

- Watch the queue, accept, prepick, ban, pick and lock in from your phone over LAN or Tailscale; see [docs/phone-remote.md](docs/phone-remote.md).

**Diagnostics**

- **Connection checks** (Settings → About) copies a sanitized report for bug reports.

## Docs

- [Features](docs/features.md)
- [Accounts](docs/accounts.md)
- [Phone remote](docs/phone-remote.md)
- [Contributing](CONTRIBUTING.md)
- [Security and privacy](SECURITY.md)
- [Open-source credits and licenses](docs/credits.md)
- [Releasing](docs/RELEASING.md)

Swapper's own source code is licensed under the [MIT License](LICENSE).

---

Swapper was created under Riot Games' ["Legal Jibber Jabber" policy](https://www.riotgames.com/en/legal) using assets owned by Riot Games. Riot Games does not endorse or sponsor this project.
