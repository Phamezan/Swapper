# Swapper

Swapper is a free Windows tray app for switching between saved Riot Client sign-ins. It uses the official Riot Client for sign-in and never asks for your password. In champion select it sets your runes, summoner spells and item build in one click, and its phone remote follows League's queue and champion select over your private local network or Tailscale.

## Download

Download the Windows installer from the [latest GitHub release](https://github.com/Phamezan/Swapper/releases/latest). From v0.4 on, Swapper checks for updates itself and installs them with **Update & Restart**; older versions need v0.4 installed by hand once. Exit an older Swapper from the tray before installing it. The installer is currently unsigned, so Windows may ask you to confirm that you trust it. Swapper stores its settings and encrypted sessions under `%LOCALAPPDATA%\Swapper`; reinstalling the app does not remove them.

## Demo

https://github.com/user-attachments/assets/0b5bcc1b-b6f0-492a-853b-2f47ef7b6417

A 26-second tour of v0.4 (also as an [MP4 file](docs/media/swapper-v0.4.mp4)). The account names are samples.

## Features

**Accounts**

- Save multiple Riot Client sign-ins and switch between them from the tray flyout. Swapper closes Riot and League client processes before restoring the selected session; it refuses to switch while a game is running.
- Detect the signed-in Riot account automatically and avoid duplicate entries using Riot's account identifier. Give saved accounts optional nicknames and remove them from the tray app.
- **Repair Account** when a saved session has expired or can't be restored: sign in again in Riot Client and Swapper replaces the saved session after checking it is the same account. After each switch Swapper watches for Riot's login screen and offers the repair when it appears.
- **Desktop shortcuts** that switch straight to one account (`swapper://switch/…` links), and an optional **global hotkey** that opens the flyout.
- Start Swapper when you sign in to Windows, and pin the tray flyout open while you use it. Optionally launch through the bundled [Deceive](https://github.com/molenzwiebel/Deceive) executable to use Deceive's presence behavior.

**Champion select**

- Pick a recommended rune preset for your champion and role, copy a pro's page from the **Pro builds** tab, or edit every rune yourself in the **Editor** — from the tray flyout or the phone. The role follows champion select; the role buttons override it. A rank filter adjusts the [op.gg](https://op.gg) recommendation bracket.
- Presets can also set their summoner spells and, with **Import item build** on, add an item set to the League in-game shop: starter items, the full build, situational options grouped by purpose, and the ability max order in its title (for example `Swapper: Ahri · Max Q > W > E`). Swapper keeps only one of its own item sets, removes it after the game, and never touches yours.
- Auto-applying the recommended preset when your champion locks in is a setting, off by default.
- Windows notifications when a ready check pops and when champion select starts, each with its own toggle.
- When op.gg, lolalytics or u.gg is slow or down, Swapper shows the last data it fetched, labelled with its age.

**Phone remote**

- Use your phone over a trusted private Wi-Fi or Ethernet network, without Tailscale, to watch queue time, accept a match, prepick, ban, pick, and lock in a champion. Tailscale remains an optional transport. Champion search, role filters, and icons come from the running League client. The phone vibrates and chimes on a ready check while the page is open.
- Pair a phone by scanning a single-use QR code. Paired phones stay paired across restarts; rename or revoke them in **Settings → Remote**.

**Diagnostics**

- **Swapper Doctor** (Settings → About) checks Riot Client, League Client, the data providers, Remote Control and the installed version, and copies a sanitized report for bug reports.

## Add and switch accounts

1. Open Swapper. Right-click its tray icon and choose **Add Account**, or open the flyout and choose **Add account**.
2. Sign in through the official Riot Client with **Stay signed in** selected. League does not need to be open. Swapper detects the Riot account, then offers **Save Account**. If that same account is already saved, it offers **Update Saved Account**.
3. Left-click the Swapper tray icon to open your accounts. Left-click an account to switch. Right-click an account for **Switch to account**, **Edit nickname**, **Remove account**, and **Add account**. Removing an account asks for confirmation.

Do not use Riot Client's **Sign out** between saved accounts; it can invalidate a session that Swapper needs to restore. Use Swapper's **Sign in to another account** flow. If a saved session expires, sign back into that same account in Riot Client and update its saved session. Accounts created by older Swapper builds without a stored account identifier cannot be verified automatically; save a new verified entry before removing the old one.

## Phone Remote Control over LAN

1. Connect the PC and phone to the same trusted private Wi-Fi or Ethernet network. In Swapper **Settings → Remote**, turn on **Remote Control** and leave the transport on **LAN**.
2. The first time, the Remote tab shows **Allow on private networks**. Click it and approve the Windows prompt, which names Swapper; it adds a Windows Firewall rule for Private networks only. Swapper never asks on its own, and it starts no PowerShell or command windows. Swapper only offers LAN access when Windows identifies the active network as **Private** and selects an active default-route Ethernet or Wi-Fi interface. It does not advertise an address on a Public network.
3. Press **QR** and scan it with the phone. Pairing links work once and expire after five minutes. The phone stays paired, also after Swapper or the PC restarts, until you revoke it in the paired devices list or press **Reset LAN Access**, which unpairs every phone and shows a fresh QR.
4. The phone opens the existing remote page. League Client must be running for queue and champion-select controls to be available.

LAN uses HTTP and is meant for trusted private networks. Do not use it on public or untrusted Wi-Fi. No router configuration, IP entry, or pairing code is needed.

A paired phone is tied to the PC's local address. If that address changes (for example after a router restart), Swapper shows **Your phone needs to reconnect** with the new QR code; scan it once and remove the old entry. Swapper also advertises itself as `swapper.local`, and an iPhone can switch to that name from the phone page so it keeps working when the address changes. Android browsers can't open `.local` addresses.

## Phone Remote Control with Tailscale

1. Follow [Tailscale's quickstart](https://tailscale.com/docs/how-to/quickstart) to install Tailscale on the Windows PC and your phone. Sign both devices into the same tailnet. See Tailscale's [Windows installation guide](https://tailscale.com/docs/install/windows) if needed.
2. Enable [MagicDNS and HTTPS certificates](https://tailscale.com/docs/how-to/set-up-https-certificates) for your tailnet. Swapper uses [Tailscale Serve](https://tailscale.com/docs/features/tailscale-serve) to expose its local page to devices allowed by your tailnet policy. Swapper does not use Funnel or make the page public on the internet.
3. In Swapper **Settings**, turn on **Remote Control**, select **Tailscale**, and open the displayed link on your phone or scan its QR code. Keep Tailscale connected on both devices.
4. Open League Client on the PC. The phone page shows connection status and the queue timer. At ready check you can accept the match; during champion select you can prepick, ban, pick, and lock in when League allows those actions.

Anyone permitted by your tailnet's access rules to reach this Serve route can use the page; Swapper leaves access control to Tailscale on this transport. See Tailscale's [Serve access guidance](https://tailscale.com/docs/features/tailscale-serve) if you share a tailnet. Account switching itself remains in the Windows tray app.

## Privacy and storage

Swapper stores account names, Riot account identifiers, settings, and references to encrypted session snapshots in `%LOCALAPPDATA%\Swapper`. Session snapshots are encrypted with Windows DPAPI for the current Windows user and are not sent to the phone or a Swapper server. Riot's own live client files remain under `%LOCALAPPDATA%\Riot Games`. Copying a vault to a different Windows account generally will not make it usable there.

Swapper reads the local Riot and League clients to identify a signed-in account and provide remote controls. It does not request passwords or use a Riot developer API key. Riot may change its clients independently, so a future client update can require a Swapper update.

## Open-source credits and licenses

- [TcNo Account Switcher](https://github.com/TCNOco/TcNo-Acc-Switcher) inspired the account-switching flow and informed the session-path allowlist. Swapper is an independent Riot-focused implementation; TcNo is not bundled.
- [Mimic](https://github.com/molenzwiebel/Mimic) inspired the phone-based League lobby and champion-select controls. Mimic is not bundled.
- [LeagueAkari](https://github.com/LeagueAkari/LeagueAkari) is MIT licensed and is the reference for Swapper's op.gg client and rune-build parsing (`src-tauri/src/runes/opgg.rs`), which power the rune presets and the rune editor. LeagueAkari is not bundled. Swapper reads public [op.gg](https://op.gg) champion statistics for rune recommendations; it is not affiliated with op.gg.
- Each preset rune page also shows the 6 items players build with that keystone, and the imported item set's options and skill order, read from public [lolalytics](https://lolalytics.com) build pages (`src-tauri/src/runes/lolalytics.rs`). Swapper fetches at most one page per keystone, caches it for 30 minutes, and shows the sample size when it is small; the data is a third-party aggregate and the site is **unofficial** and can change. Swapper is not affiliated with lolalytics.
- The **Pro builds** tab reads [probuildstats](https://probuildstats.com) / [u.gg](https://u.gg) (same company) through their public, **unofficial** GraphQL endpoint for pros' solo-queue games (`src-tauri/src/runes/probuilds.rs`). Swapper sends at most one request per champion per champion select, cached, with a short timeout; the endpoint is undocumented and can change or be unavailable. Swapper is not affiliated with probuildstats or u.gg. No pro images are hotlinked; only names, teams and leagues are shown as text.
- [Deceive v1.18.0](https://github.com/molenzwiebel/Deceive/releases/tag/v1.18.0) is bundled as an **unmodified, separate executable**. Deceive is GPL-3.0 licensed; its [license](src-tauri/resources/Deceive-LICENSE.txt) and [source information](src-tauri/resources/Deceive-SOURCE.txt) ship with Swapper. Swapper does not claim authorship of Deceive.
- [Accshift](https://github.com/klNuno/accshift) was another reference for Riot session paths; it is not bundled.

Swapper's own source code is licensed under the [MIT License](LICENSE). That license does not relicense Deceive, Riot-owned content, or other third-party software.

## Development

Requires Windows, Node.js, Rust with the MSVC toolchain, and the [Tauri 2 Windows prerequisites](https://v2.tauri.app/start/prerequisites/).

```powershell
npm ci
npm run tauri dev
```

`npm run build` checks and bundles the frontend. See [docs/RELEASING.md](docs/RELEASING.md) for publishing a release. `cargo test --manifest-path src-tauri/Cargo.toml --locked` runs native tests. To make a Windows installer locally, run `npm run tauri -- build`; a release also needs a manual test with Riot Client and multiple accounts.

During `npm run tauri dev`, the phone page is proxied to the Vite dev server instead of being served from the bundled `dist/` assets, so it stays in step with the desktop UI on every reload. Release builds serve the bundled assets as usual.

## Contributing

Bug reports, documentation fixes, and code contributions are welcome. For a bug, open a GitHub issue with the Swapper version, Windows version, steps to reproduce, and what happened. Remove Riot account identifiers, session files, access tokens, and other private data from screenshots and logs before sharing them.

For code changes, open an issue first if the change is substantial, then work on a focused branch and submit a pull request. Explain the behavior you changed and how you tested it. Run `npm run build` and `cargo test --manifest-path src-tauri/Cargo.toml --locked` locally; for Riot Client, League Client, account switching, or phone remote changes, include the manual scenarios you tested. Automated GitHub checks may not run on every pull request, so include your local results.

---

Swapper was created under Riot Games' ["Legal Jibber Jabber" policy](https://www.riotgames.com/en/legal) using assets owned by Riot Games. Riot Games does not endorse or sponsor this project.
