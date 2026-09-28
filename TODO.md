# TODO

Before implementing a task:

- Read the **Scope Guardrails** and the matching **Minimum Swapper Implementations** and **Feature-by-feature recommendation** sections in `docs/research/swapper-open-source-references.md`.
- Check each project's recommendation and license caveat. Inspect Swapper's existing code for reusable pieces first.
- In your plan, list relevant references and say whether each is a dependency, code-reuse candidate, or design/architecture inspiration.
- Verify the upstream license for the exact code and revision before copying, and preserve required notices. Treat unclear or unlicensed code as reference-only.

## P0  Reliability and Product Polish

### Persistent Remote Control pairing
- [x] Persist paired LAN devices across Swapper restarts.
- [x] Replace restart-scoped LAN sessions with durable device credentials.
- [x] Store pairing credentials securely on the PC.
- [x] Add a **Paired devices** section in Settings.
- [x] Show device name and last-seen time.
- [x] Allow individual paired devices to be revoked.
- [x] Keep **Reset LAN Access** as a way to revoke every paired device.
- [x] Keep QR pairing as the first-time trust establishment flow.
- [x] Ensure pairing credentials are never exposed in logs or diagnostics.

### LAN discovery with mDNS
- [x] Advertise Swapper's LAN Remote Control service through mDNS.
- [x] Give the PC a stable local discovery name such as `swapper.local`.
- [x] Let previously paired phones rediscover the PC after its LAN IP changes.
- [x] Keep authentication separate from discovery; mDNS must not expose pairing credentials.
- [x] Fall back gracefully to the current IP-based connection if mDNS is unavailable.

### Swapper Doctor / diagnostics
- [x] Add a **Swapper Doctor** section in Settings.
- [x] Report Riot Client connection status.
- [x] Report League Client / LCU connection status.
- [x] Report active account session health.
- [x] Report OP.GG availability and latency.
- [x] Report Lolalytics availability and latency.
- [x] Report ProBuildStats / U.GG availability and latency.
- [x] Report LAN Remote Control status and selected interface.
- [x] Report Tailscale availability when selected.
- [x] Report bundled Deceive availability/version.
- [x] Report Swapper version/update status.
- [x] Provide actionable recovery messages instead of raw backend errors.
- [x] Add **Retry** actions where appropriate.
- [x] Add **Copy diagnostics** with sanitized diagnostic information.
- [x] Never include PUUIDs, Riot IDs, session data, access tokens, pairing secrets, cookies or encrypted vault contents in copied diagnostics.

### Provider resilience and cached fallback
- [ ] Introduce provider abstractions instead of coupling rune/build features directly to individual websites.
- [ ] Separate recommendation data from OP.GG-specific implementation details.
- [ ] Separate build data from Lolalytics-specific implementation details.
- [ ] Cache the last successfully fetched recommendations.
- [ ] Cache the last successfully fetched item builds.
- [ ] Continue showing cached data when a provider is temporarily unavailable.
- [ ] Clearly label stale data with its last-updated timestamp.
- [ ] Add fallback providers where reliable alternatives exist.
- [ ] Distinguish provider outage, timeout and response-format changes in diagnostics.

### Automatic updates
- [x] Add update checking against GitHub Releases.
- [x] Check periodically rather than only on app startup.
- [x] Show the installed and available version.
- [x] Display release notes before updating.
- [x] Add **Update & Restart**.
- [x] Ensure account vault/session data survives application updates.
- [x] Handle failed updates without leaving Swapper unusable.

### Signed Windows releases
- [ ] Investigate free OSS certificate such as https://ossign.org/ or https://signpath.org/
- [ ] Sign Swapper executables and installers.
- [ ] Verify whether bundled `Deceive.exe` affects signing requirements.
- [ ] Integrate signing into the release workflow.
- [ ] Remove unsigned-installer guidance from the README once releases are consistently signed.

---

## P1  User Experience

### Account session repair flow
- [ ] Detect when a saved Riot session has expired or can no longer be restored.
- [ ] Replace generic restore failures with a **Repair Account** action.
- [ ] Launch Riot Client for the repair flow.
- [ ] Ask the user to sign into the affected account normally.
- [ ] Detect the signed-in Riot identity automatically.
- [ ] Verify that the detected PUUID matches the saved account.
- [ ] Replace the expired saved session without creating a duplicate account.
- [ ] Return directly to the normal account list when repair succeeds.

### Ready-check and champion-select notifications
- [ ] Show a Windows notification when a ready check starts.
- [ ] Show a notification when champion select begins.
- [ ] Avoid duplicate notifications for the same event.
- [ ] Add notification settings.
- [ ] Allow ready-check notifications to be disabled independently.
- [ ] Consider optional sound.
- [ ] Add phone vibration/sound cues in the Remote Control UI where browser support allows it.

### Global Swapper hotkey
- [x] Add an optional global keyboard shortcut to open/toggle the tray flyout.
- [x] Make the shortcut configurable.
- [x] Detect shortcut conflicts.
- [x] Allow the hotkey to be disabled.

### Account launch shortcuts
- [ ] Add a Swapper command/protocol for switching to a specific saved account.
- [ ] Support creating desktop shortcuts for individual accounts.
- [ ] Keep account identifiers internal rather than embedding sensitive session data in shortcuts.
- [ ] Refuse shortcut-triggered switching while a game is running using the same safety checks as the normal UI.

---

### Remote Control device UX
- [x] Give paired phones readable names where possible.
- [x] Show whether each paired device is currently connected.
- [x] Show last-seen timestamps.
- [x] Allow renaming paired devices locally.
- [x] Show a subtle notification when a new device is paired.

---

## Release Quality

### PR #2 / v0.4 real-device validation
- [ ] Test LAN Remote Control on a Windows **Private** network.
- [ ] Verify LAN Remote Control stays unavailable on a Windows **Public** network.
- [ ] Test Windows Firewall first-run behavior.
- [ ] Test Wi-Fi.
- [ ] Test Ethernet.
- [ ] Test switching between Wi-Fi and Ethernet.
- [ ] Test DHCP/IP address changes.
- [ ] Test pairing from a real iPhone.
- [ ] Test pairing from a real Android phone if available.
- [ ] Test pairing-token expiry.
- [ ] Test reuse of an already-consumed pairing token.
- [ ] Test **Reset LAN Access**.
- [ ] Test two paired phones.
- [ ] Test Swapper restart behavior.
- [ ] Regression-test Tailscale Remote Control.
- [ ] Test ready check from the phone.
- [ ] Test champion prepick, ban, pick and lock-in from the phone.
- [ ] Test rune presets from the desktop.
- [ ] Test rune presets from the phone.
- [ ] Test manual rune editing.
- [ ] Test summoner-spell application.
- [ ] Test role override in Practice Tool / modes without assigned roles.
- [ ] Test rank filter.
- [ ] Test Pro Builds.
- [ ] Test preset item-set import in the real League in-game shop.
- [ ] Test Pro Build item-set import in the real League in-game shop.
- [ ] Test account switching with multiple real Riot accounts.
- [ ] Test expired-session behavior.
- [ ] Test switching with League currently running.
- [ ] Test switching while an actual game process is running.
- [ ] Test launching through Deceive.
- [ ] Confirm all failure cases produce useful user-facing messages rather than raw errors.
