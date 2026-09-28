# TODO

Before implementing a task:

- Read the **Scope Guardrails** and the matching **Minimum Swapper Implementations** and **Feature-by-feature recommendation** sections in `docs/research/swapper-open-source-references.md`.
- Check each project's recommendation and license caveat. Inspect Swapper's existing code for reusable pieces first.
- In your plan, list relevant references and say whether each is a dependency, code-reuse candidate, or design/architecture inspiration.
- Verify the upstream license for the exact code and revision before copying, and preserve required notices. Treat unclear or unlicensed code as reference-only.

## P0  Reliability and Product Polish

### Persistent Remote Control pairing
- [ ] Persist paired LAN devices across Swapper restarts.
- [ ] Replace restart-scoped LAN sessions with durable device credentials.
- [ ] Store pairing credentials securely on the PC.
- [ ] Add a **Paired devices** section in Settings.
- [ ] Show device name and last-seen time.
- [ ] Allow individual paired devices to be revoked.
- [ ] Keep **Reset LAN Access** as a way to revoke every paired device.
- [ ] Keep QR pairing as the first-time trust establishment flow.
- [ ] Ensure pairing credentials are never exposed in logs or diagnostics.

### LAN discovery with mDNS
- [ ] Advertise Swapper's LAN Remote Control service through mDNS.
- [ ] Give the PC a stable local discovery name such as `swapper.local`.
- [ ] Let previously paired phones rediscover the PC after its LAN IP changes.
- [ ] Keep authentication separate from discovery; mDNS must not expose pairing credentials.
- [ ] Fall back gracefully to the current IP-based connection if mDNS is unavailable.

### Swapper Doctor / diagnostics
- [ ] Add a **Swapper Doctor** section in Settings.
- [ ] Report Riot Client connection status.
- [ ] Report League Client / LCU connection status.
- [ ] Report active account session health.
- [ ] Report OP.GG availability and latency.
- [ ] Report Lolalytics availability and latency.
- [ ] Report ProBuildStats / U.GG availability and latency.
- [ ] Report LAN Remote Control status and selected interface.
- [ ] Report Tailscale availability when selected.
- [ ] Report bundled Deceive availability/version.
- [ ] Report Swapper version/update status.
- [ ] Provide actionable recovery messages instead of raw backend errors.
- [ ] Add **Retry** actions where appropriate.
- [ ] Add **Copy diagnostics** with sanitized diagnostic information.
- [ ] Never include PUUIDs, Riot IDs, session data, access tokens, pairing secrets, cookies or encrypted vault contents in copied diagnostics.

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
- [ ] Add update checking against GitHub Releases.
- [ ] Check periodically rather than only on app startup.
- [ ] Show the installed and available version.
- [ ] Display release notes before updating.
- [ ] Add **Update & Restart**.
- [ ] Ensure account vault/session data survives application updates.
- [ ] Handle failed updates without leaving Swapper unusable.

### Signed Windows releases
- [x] Investigate free OSS certificate such as https://ossign.org/ or https://signpath.org/
- [ ] Sign Swapper executables and installers.
- [x] Verify whether bundled `Deceive.exe` affects signing requirements.
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
- [ ] Add an optional global keyboard shortcut to open/toggle the tray flyout.
- [ ] Make the shortcut configurable.
- [ ] Detect shortcut conflicts.
- [ ] Allow the hotkey to be disabled.

### Account launch shortcuts
- [ ] Add a Swapper command/protocol for switching to a specific saved account.
- [ ] Support creating desktop shortcuts for individual accounts.
- [ ] Keep account identifiers internal rather than embedding sensitive session data in shortcuts.
- [ ] Refuse shortcut-triggered switching while a game is running using the same safety checks as the normal UI.

---

### Remote Control device UX
- [ ] Give paired phones readable names where possible.
- [ ] Show whether each paired device is currently connected.
- [ ] Show last-seen timestamps.
- [ ] Allow renaming paired devices locally.
- [ ] Show a subtle notification when a new device is paired.

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
