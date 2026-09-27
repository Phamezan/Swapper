# TODO

## Fixes

- [ ] **Hide the Accounts button in the rune view.** The "Accounts" button is still visible on the rune screen and should not be shown.
- [ ] **Let the user change the rune role.** Rune presets, builds and pro builds follow the assigned position from champion select. In modes without one, such as Practice Tool, the detected role is always jungle. Add a role switcher on the rune screen so the user can pick another role.

## Release

- [ ] **Release the new version.** Bump the version in `package.json`, `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json`, and update the README for the new features:
  - rune presets and the rune editor
  - pro builds
  - summoner spells
  - per-keystone item builds
  - the rank filter
  - Start on startup
  - the pinned flyout

  Add the op.gg, probuildstats/u.gg and lolalytics credits.

## Features

- [ ] **Import builds into the game.** Write the selected preset or pro build's items into the League client as an item set, so it appears in the in-game shop.
- [ ] **Remote Control over LAN, without Tailscale.** Serve the phone page on the local network for users who don't have Tailscale. Swapper shows a QR code that contains the LAN address plus a generated access token, so there is no code to type in (unlike Mimic). The token authorizes the phone; the page must stay unreachable without it.
  - Open questions: how long a token stays valid, and whether it can be revoked or reset.
  - Open questions: HTTP vs HTTPS on the LAN, and the certificate for HTTPS.
  - Open questions: the Windows firewall prompt, and which network interface/IP to advertise.
