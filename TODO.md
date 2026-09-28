# TODO

## Fixes

- [x] **Hide the Accounts button in the rune view.** The "Accounts" button is still visible on the rune screen and should not be shown.
- [x] **Let the user change the rune role.** Rune presets, builds and pro builds follow the assigned position from champion select. In modes without one, such as Practice Tool, the detected role is always jungle. Add a role switcher on the rune screen so the user can pick another role.

## Release

- [x] **Release the new version.** Bump the version in `package.json`, `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json`, and update the README for the new features:
  - rune presets and the rune editor
  - pro builds
  - summoner spells
  - per-keystone item builds
  - the rank filter
  - Start on startup
  - the pinned flyout

  Add the op.gg, probuildstats/u.gg and lolalytics credits.

## Features

- [x] **Import builds into the game.** Write the selected preset or pro build's items into the League client as an item set, so it appears in the in-game shop.
- [x] **Remote Control over LAN, without Tailscale.** Serve the existing phone page on a dedicated authenticated LAN listener. Keep Tailscale on the loopback listener. LAN pairing uses a single-use QR token that expires after five minutes; the paired session lasts until Swapper exits, Remote Control is disabled, or **Reset LAN Access** is used.
  - Decision: use HTTP on trusted private networks. Check the active default-route Ethernet/Wi-Fi adapter and require the Windows Private profile. Ignore virtual/tunnel adapters. Add a Private-only Windows Firewall rule when LAN is enabled; do not start the LAN listener if the firewall request is denied.
