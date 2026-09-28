# Swapper — Open-Source Reference Projects

This document maps Swapper's roadmap items to open-source projects worth studying, integrating, or using as implementation references.

The key distinction is:

- **Integrate directly** — suitable as a dependency or infrastructure component.
- **Take inspiration** — study architecture, UX, or implementation patterns and build a Swapper-specific version.
- **Reference only** — useful for edge cases or ideas, but not appropriate to copy directly due to licensing, architecture mismatch, or project scope.

---

## Summary

| Project | Relevant Swapper TODOs | What is worth studying | Recommendation | License / caveat |
|---|---|---|---|---|
| **Tauri plugins-workspace** | Auto-update, global hotkey, account shortcuts/deep links, notifications | Official updater, global-shortcut, deep-link, process, and notification plugins | **Integrate directly** | MIT / Apache-2.0 |
| **keepsimple1/mdns-sd** | mDNS LAN discovery | Rust DNS-SD advertiser/browser, Windows support, IPv4/IPv6 | **Integrate directly** | MIT / Apache-2.0 |
| **LocalSend** | LAN discovery, persistent devices, network UX, firewall UX, signing | Local-first discovery, device identity, HTTPS, troubleshooting, signed Windows releases | **Take strong inspiration** | Apache-2.0 |
| **KDE Connect** | Persistent pairing, paired-device list, revoke, reconnect | Trusted-device lifecycle, certificate identity, pair/unpair flows | **Architecture inspiration** | GPL / mixed licensing |
| **Syncthing** | Persistent pairing, device identity, discovery | Device identity separate from address; rediscovery after IP changes | **Strong architecture inspiration** | MPL-2.0 |
| **PairDrop** | QR pairing, persistent phone pairing, paired-device UX | Browser-friendly pairing and reconnect UX | **UX inspiration only** | GPL-3.0 |
| **BlueBottle LeagueBroadcast Companion module** | mDNS discovery, remote authentication | League-specific zero-config discovery + pairing-token auth | **Study closely** | MIT |
| **Tailscale** | Swapper Doctor, diagnostics, health reporting | Structured health checks, diagnostics, support reports | **Architecture inspiration** | BSD-3-Clause |
| **LeagueAkari** | Self-update, LCU robustness, provider/client abstraction | Mature League-client architecture and update state handling | **Study heavily** | MIT |
| **league-lean** | Provider fallback, cached/fallback rune data, update checker | Lolalytics → U.GG fallback, local LCU assets, update checking | **Take implementation inspiration** | MIT |
| **TcNo Account Switcher** | Streamer mode, account shortcuts, custom protocol, updater | Privacy mode, shortcuts, tray UX, protocol-driven switching | **Product/UX inspiration only** | GPL-3.0 |
| **Mimic** | Phone remote, ready check/champ select remote | WebSocket state flow, remote champion select edge cases | **Reference only** | MIT, archived |
| **RabadonGG** | Update UX, changelog, global hotkey, tray UX | Update banner, What's New panel, configurable hotkey | **Visual/product inspiration only** | No declared license |
| **LocalSend + SignPath workflow** | Signed Windows releases | GitHub Actions → SignPath → signed Windows artifacts | **Copy workflow pattern** | Depends on SignPath eligibility |

---

---

# Scope Guardrails

Swapper should stay a small, focused Windows utility. The projects in this document are references for proven ideas, not blueprints to reproduce in full.

For every borrowed idea, prefer the smallest implementation that solves Swapper's exact problem.

## General rules

- Do not introduce a generic plugin system unless Swapper has multiple concrete plugin use cases.
- Do not build a service framework where a small module or trait is enough.
- Do not add telemetry, cloud infrastructure, accounts, relays, or hosted services unless a future feature genuinely requires them.
- Do not reproduce another project's architecture just because it is mature.
- Prefer official Tauri functionality over custom infrastructure.
- Prefer one-purpose Rust crates over adopting large frameworks.
- Keep Remote Control local-first: LAN by default, Tailscale optional.
- Avoid background services outside the Swapper process.
- Avoid adding persistent state unless the feature genuinely needs it.
- New abstractions should exist because Swapper already has at least two concrete implementations or because they isolate a fragile external dependency.

---

# Minimum Swapper Implementations

## Persistent Remote Control pairing

### Minimum implementation

- Generate one high-entropy credential per paired phone.
- Store the trusted-device record locally.
- Store enough metadata to show a readable device name and last-seen time.
- Allow individual revoke.
- Keep **Reset LAN Access** as revoke-all.
- Let paired phones reconnect after Swapper restarts.

### Do not implement

- Public-key infrastructure.
- Certificate authorities.
- Full KDE Connect-style certificate negotiation.
- User accounts.
- Cloud device synchronization.
- Device-to-device trust graphs.
- Remote pairing outside LAN/Tailscale.

The useful lesson from KDE Connect and Syncthing is the trust lifecycle, not their full cryptographic architecture.

---

## mDNS discovery

### Minimum implementation

Use `mdns-sd` to advertise one Swapper service, for example:

```text
_swapper._tcp.local
```

Expose only enough metadata to identify the installation and protocol version.

The phone should use discovery to answer:

> Where is the Swapper PC I already trust?

### Do not implement

- A general-purpose discovery framework.
- Global discovery servers.
- Relay infrastructure.
- Custom DNS servers.
- Peer-to-peer routing.
- Multiple discovery protocols unless mDNS proves insufficient.

Syncthing is useful here only for the principle that **device identity is separate from network address**.

---

## Swapper Doctor

### Minimum implementation

A single diagnostic view with a small set of checks:

```text
Riot Client
League Client / LCU
Saved account session
OP.GG
Lolalytics
ProBuildStats / U.GG
LAN Remote Control
Tailscale
Deceive
Swapper update status
```

Each check should return something like:

```text
Healthy
Unavailable
Timed out
Not configured
Needs attention
```

Provide a short user-facing explanation and, where useful, a Retry button.

Add **Copy diagnostics** with sanitized information.

### Do not implement

- Telemetry collection.
- Remote observability.
- Metrics backends.
- Crash-reporting infrastructure.
- Distributed tracing.
- Log upload servers.
- Automated support-ticket submission.
- Huge diagnostic archives.

Tailscale is an architecture reference for clear subsystem health, not a model for Swapper's infrastructure size.

---

## Provider resilience

### Minimum implementation

Introduce only enough abstraction to prevent the UI from depending directly on one website.

For example:

```rust
trait RuneProvider {
    async fn recommendations(...) -> Result<Recommendations>;
}
```

Start with the providers Swapper actually uses.

Add:

- last-successful-result cache
- stale timestamp
- one fallback where it materially improves reliability

### Do not implement

- Dynamic provider plugins.
- Runtime provider discovery.
- User-installable providers.
- Dependency injection frameworks.
- Generic scraping engines.
- Provider marketplace/configuration UI.
- Complex scoring across five data sources.

The goal is resilience against a provider changing, not turning Swapper into a data aggregation platform.

---

## Automatic updates

### Minimum implementation

Use the official Tauri updater.

Provide:

```text
Update available
Version
Short release notes

[ Update & Restart ]
```

### Do not implement

- A custom updater.
- Binary-diff patch infrastructure.
- A custom CDN.
- A release server.
- TcNo-style patching unless full installers become a proven problem.

LeagueAkari and TcNo are UX references. Tauri should remain the implementation.

---

## Global hotkey

### Minimum implementation

Use the official Tauri global-shortcut plugin.

Support one configurable shortcut for opening/toggling Swapper.

### Do not implement

- A general macro system.
- Multiple action bindings.
- Scripting.
- Key-sequence recording.
- Per-account hotkey management unless users actually request it.

---

## Account launch shortcuts

### Minimum implementation

Use Tauri deep links or a small command-line entry point.

Example:

```text
swapper://switch/<internal-account-id>
```

The identifier should refer to an internal saved account only.

### Do not implement

- A general command protocol.
- Remote scripting.
- Arbitrary command execution.
- Exposing session data in URLs.
- An automation API before concrete use cases exist.

TcNo proves that account shortcuts are useful; Swapper does not need TcNo's broader command system.

---

## Privacy mode

### Minimum implementation

One toggle that hides or masks:

- Riot IDs
- nicknames if desired
- profile information
- LAN address
- pairing QR
- remote access information

### Do not implement

- OBS/XSplit process detection.
- Automatic scene awareness.
- Stream-platform integrations.
- Per-field privacy rule editors.

Automatic streamer detection can be revisited if there is actual demand.

---

## Signed Windows releases

### Minimum implementation

Use an existing signing provider such as SignPath if Swapper qualifies.

The pipeline should remain:

```text
GitHub Actions
    ↓
Build Tauri artifacts
    ↓
Submit for signing
    ↓
Publish signed artifacts
```

### Do not implement

- A custom signing service.
- Certificate-management infrastructure.
- Custom artifact distribution.
- Separate updater/signing backend.

---

## Account repair

### Minimum implementation

Reuse Swapper's existing identity and vault functionality:

```text
Restore fails
   ↓
Repair Account
   ↓
Open Riot Client
   ↓
User signs in
   ↓
Detect PUUID
   ↓
Verify same account
   ↓
Replace saved session snapshot
```

### Do not implement

- A new account database.
- Password storage.
- Riot OAuth flows.
- Account recovery services.
- Cloud account synchronization.

No external dependency is needed.

---

## Ready-check and champion-select notifications

### Minimum implementation

Use Swapper's existing Tauri notification support.

Notify on:

- ready check
- champion select beginning

Optionally add a small setting for sound/notification enablement.

### Do not implement

- Notification rules engine.
- Discord integration.
- Push-notification backend.
- Mobile push service.
- Event automation framework.

The phone can use browser vibration/sound only while the Remote Control page is active if supported.

---

# Implementation Priority With Complexity Limits

## P0

### Persistent pairing
Target: small local trusted-device store.

**Complexity limit:** no PKI, cloud, or external pairing service.

### mDNS
Target: one advertised service using `mdns-sd`.

**Complexity limit:** no generic discovery layer.

### Swapper Doctor
Target: roughly 8–10 explicit health checks.

**Complexity limit:** no telemetry infrastructure.

### Provider resilience
Target: isolate fragile providers + cache + one sensible fallback.

**Complexity limit:** no provider plugin ecosystem.

### Automatic updater
Target: official Tauri updater.

**Complexity limit:** no custom patching system.

### Signed releases
Target: CI integration with an existing signing provider.

**Complexity limit:** no custom signing infrastructure.

---

## P1

### Account repair
Target: reuse existing PUUID detection and vault replacement.

### Notifications
Target: two or three League lifecycle events only.

### Global hotkey
Target: one configurable Swapper toggle shortcut.

### Account shortcuts
Target: one safe deep-link/CLI switching command.

---

## P2

### Privacy mode
Target: one toggle and simple masking.

### Paired-device polish
Target: name, online state, last seen, revoke.

Do not add advanced device management until there is evidence it is needed.

---

---

# 1. Tauri plugins-workspace

Repository:

`https://github.com/tauri-apps/plugins-workspace`

## Relevant Swapper tasks

- Automatic updates
- Global Swapper hotkey
- Account launch shortcuts
- Deep-link / protocol support
- Notifications

## Recommendation

**Integrate directly.**

Swapper already uses Tauri, so these features should use the official Tauri plugins rather than custom implementations.

Especially relevant plugins:

- `updater`
- `global-shortcut`
- `deep-link`
- `notification`
- `process`

For example, account shortcuts could eventually invoke:

```text
swapper://switch/<internal-account-id>
```

The shortcut should contain only Swapper's internal account identifier, never Riot session information.

---

# 2. keepsimple1/mdns-sd

Repository:

`https://github.com/keepsimple1/mdns-sd`

## Relevant Swapper tasks

- mDNS LAN discovery
- Automatic rediscovery after the PC's LAN IP changes
- Persistent Remote Control pairing

## Recommendation

**Integrate directly.**

This is probably the best actual dependency for Swapper's Rust backend.

Swapper could advertise something similar to:

```text
_swapper._tcp.local

instance = Gaming-PC
port = 38472
version = 1
deviceId = 74e8...
```

The important part is that the `deviceId` identifies the Swapper installation while mDNS only tells the phone where that device currently lives.

---

# 3. LocalSend

Repository:

`https://github.com/localsend/localsend`

Protocol:

`https://github.com/localsend/protocol`

## Relevant Swapper tasks

- LAN discovery
- Persistent devices
- Local-network UX
- Firewall / troubleshooting UX
- Signed Windows releases

## Why it is useful

LocalSend is one of the best references for a polished local-first desktop/mobile application.

Useful areas to study:

- automatic local discovery
- separating device identity from IP address
- HTTPS between local devices
- local-network troubleshooting
- device naming
- reconnection after network changes
- release signing

## Recommendation

**Take strong architecture and UX inspiration.**

Do not integrate LocalSend itself.

---

# 4. KDE Connect

Repository:

`https://github.com/KDE/kdeconnect-kde`

## Relevant Swapper tasks

- Persistent Remote Control pairing
- Paired-device list
- Revoke device
- Automatic reconnect

## Why it is useful

KDE Connect is a strong reference for the full trust lifecycle:

```text
Unknown device
    ↓
Pair
    ↓
Trusted device
    ↓
Reconnect automatically
    ↓
Revoke
    ↓
Unknown device
```

It also validates reconnecting devices against persistent certificate identity rather than trusting the current IP address.

## Recommendation

**Architecture inspiration only.**

Do not copy implementation directly into Swapper because KDE Connect uses GPL / mixed licensing.

---

# 5. Syncthing

Repository:

`https://github.com/syncthing/syncthing`

## Relevant Swapper tasks

- Persistent pairing
- Stable device identity
- LAN rediscovery
- Handling DHCP/IP changes

## Most useful concept

Syncthing clearly separates:

```text
Device identity
      ≠
Network address
```

A paired device remains the same device even when its IP changes.

Discovery only answers:

> Where is this known device currently reachable?

That is exactly the model Swapper should use for persistent Remote Control.

## Recommendation

**Strong architecture inspiration.**

---

# 6. PairDrop

Repository:

`https://github.com/schlagmichdoch/PairDrop`

## Relevant Swapper tasks

- QR pairing
- Persistent phone pairing
- Paired-device UX
- Browser/mobile reconnection

## Why it is useful

PairDrop is especially useful because Swapper's remote client is also browser-based.

Useful UX flow:

```text
Scan QR
   ↓
Pair once
   ↓
Phone becomes "Michael's iPhone"
   ↓
Automatically reconnect later
```

## Recommendation

**UX inspiration only.**

PairDrop is GPL-3.0, so avoid copying implementation into Swapper's MIT codebase.

---

# 7. BlueBottle LeagueBroadcast Companion module

Repository:

`https://github.com/BlueBottleGG/companion-module-bluebottle-leaguebroadcast`

## Relevant Swapper tasks

- mDNS discovery
- Remote authentication
- Zero-configuration LAN control

## Why it is particularly relevant

This is one of the closest conceptual matches to Swapper because it combines:

- League-related remote control
- mDNS discovery
- pairing-token authentication
- zero-config local networking

The architecture is approximately:

```text
Swapper PC
   ↓ mDNS
Phone discovers Swapper
   ↓
Pairing credential
   ↓
Authenticated remote control
```

## Recommendation

**Study closely.**

It is MIT licensed, so narrowly reusing implementation ideas or code may be possible with proper attribution.

---

# 8. Tailscale

Repository:

`https://github.com/tailscale/tailscale`

## Relevant Swapper tasks

- Swapper Doctor
- Network diagnostics
- Health checks
- Sanitized support reports

## Why it is useful

Tailscale handles diagnostics as structured subsystem health rather than generic error logs.

Swapper could copy that philosophy:

```text
Riot Client       Healthy
LCU               Healthy
Saved session     Healthy
OP.GG             Healthy
Lolalytics        Timeout
ProBuildStats     Healthy
LAN interface     Private · Ethernet
Remote server     Listening
Tailscale         Not selected
Deceive           Installed
Update            Current
```

## Recommendation

**Architecture inspiration only.**

Do not integrate Tailscale diagnostic internals.

---

# 9. LeagueAkari

Repository:

`https://github.com/LeagueAkari/LeagueAkari`

## Relevant Swapper tasks

- Self-update
- LCU reliability
- League-client abstraction
- Provider/client modularity
- Error handling

## Why it is useful

LeagueAkari is one of the strongest open-source examples of a mature League Client companion.

Especially useful areas:

- self-update subsystem
- update state transitions
- update progress
- League client connection handling
- modular integration structure
- configuration and persistence architecture

## Recommendation

**Study heavily.**

Because it is MIT licensed, narrow reuse may be possible where it genuinely fits.

Do not import LeagueAkari wholesale; Swapper should remain much smaller.

---

# 10. league-lean

Repository:

`https://github.com/Nicetyone/league-lean`

## Relevant Swapper tasks

- Provider fallback
- Cached rune data
- Provider resilience
- Update checking

## Why it is especially relevant

league-lean already implements the exact kind of provider fallback Swapper needs:

```text
Lolalytics
    ↓ failure
U.GG
```

It also uses:

- LCU local assets
- GitHub REST API for update checking
- multiple data sources independently

## Recommendation

**Take implementation inspiration and potentially reuse small MIT-licensed pieces.**

This is probably the first project to inspect before refactoring Swapper's rune providers.

Potential Swapper architecture:

```rust
trait RuneProvider {
    async fn recommendations(...) -> Result<Recommendations>;
}
```

With implementations such as:

```text
OpggProvider
LolalyticsProvider
UggProvider
CachedProvider
```

The UI should not care which provider supplied the data.

---

# 11. TcNo Account Switcher

Repository:

`https://github.com/TCNOco/TcNo-Acc-Switcher`

## Relevant Swapper tasks

- Privacy / streamer mode
- Account shortcuts
- Custom URL protocol
- Tray-first UX
- Automatic updates

## Useful ideas

TcNo already has:

- streamer mode
- quick-switch desktop shortcuts
- tray controls
- automatic updates
- protocol-driven commands

These map well to Swapper.

Example privacy mode:

```text
Privacy Mode [ON]

Phamezan#EUW
        ↓
••••••••#•••

LAN address
        ↓
Hidden

QR
        ↓
Hidden until explicitly revealed
```

## Recommendation

**Product/UX inspiration only.**

TcNo is GPL-3.0, so do not copy its implementation into Swapper.

---

# 12. Mimic

Repository:

`https://github.com/molenzwiebel/Mimic`

## Relevant Swapper tasks

- Ready check
- Champion select remote control
- WebSocket state updates
- Phone UX

## Why it is useful

Mimic was designed specifically around controlling League from a phone.

Its historical architecture was roughly:

```text
Phone
 ↓
Rift central server
 ↓
Desktop Conduit
 ↓
LCU
```

Swapper has a simpler architecture:

```text
Phone
 ↓
LAN / Tailscale
 ↓
Swapper
 ↓
LCU
```

## Recommendation

**Reference only.**

The most valuable thing to study is League-state edge cases:

- ready check
- champion select transitions
- action availability
- reconnection
- stale client state

Do not copy its old central relay architecture.

---

# 13. RabadonGG

Repository:

`https://github.com/Kuderic/RabadonGG`

## Relevant Swapper tasks

- Update UX
- What's New / changelog UI
- Global hotkey
- Tray UX

## Useful ideas

The project documents:

- periodic update checks
- "Update now" banner
- version display
- clickable What's New panel
- configurable global hotkey
- tray integration

## Recommendation

**Visual/product inspiration only.**

The repository currently has no declared license, so assume normal copyright and do not copy implementation.

---

# 14. LocalSend + SignPath

LocalSend code-signing documentation:

`https://github.com/localsend/localsend/blob/main/CODE_SIGNING.md`

SignPath GitHub integration:

`https://github.com/SignPath/github-action-submit-signing-request`

## Relevant Swapper tasks

- Signed Windows installer
- Signed application binaries
- Automated release signing

## Why it is useful

LocalSend is a real example of an open-source Windows application using the flow:

```text
GitHub Actions
      ↓
Build Windows artifacts
      ↓
SignPath
      ↓
Signed release
```

## Recommendation

**Reuse the workflow pattern.**

Adapt it to Swapper's Tauri release artifacts rather than inventing a signing pipeline.

---

# Feature-by-feature recommendation

## Persistent Remote Control pairing

Study together:

1. **PairDrop** — pairing UX
2. **Syncthing** — device identity
3. **KDE Connect** — trust lifecycle
4. **BlueBottle LeagueBroadcast** — League-specific pairing/auth
5. **LocalSend** — polished local-device UX

Recommended Swapper model:

```text
First pairing

Phone scans QR
   ↓
High-entropy pairing token
   ↓
Swapper creates trusted device record
   ↓
Phone receives durable device credential


Future connections

Phone discovers Swapper through mDNS
   ↓
Phone presents durable credential
   ↓
Swapper identifies known paired device
   ↓
Remote reconnects automatically
```

Do not make mDNS itself carry authentication secrets.

---

## LAN discovery

### Integrate

`keepsimple1/mdns-sd`

### Study

- LocalSend
- Syncthing
- BlueBottle LeagueBroadcast

Swapper should identify installations independently of IP address.

---

## Swapper Doctor

### Study

- Tailscale
- LeagueAkari

Recommended diagnostic categories:

```text
Application
Riot Client
League Client / LCU
Account session
Rune providers
Remote Control
Network
Tailscale
Deceive
Updater
```

Diagnostics must never contain:

- Riot session snapshots
- PUUIDs
- Riot IDs
- cookies
- pairing credentials
- access tokens
- encryption material

---

## Provider resilience

### Study first

1. league-lean
2. LeagueAkari

Recommended internal structure:

```text
Feature layer
     ↓
Provider interface
     ↓
┌──────────────┬───────────────┬─────────────┐
│ OP.GG        │ Lolalytics    │ U.GG        │
└──────────────┴───────────────┴─────────────┘
                       ↓
                  CachedProvider
```

Providers should be replaceable without changing the UI.

---

## Automatic updates

### Integrate

Official Tauri updater.

### Study UX from

- LeagueAkari
- RabadonGG
- TcNo Account Switcher

Desired UX:

```text
Swapper 0.5.1 is available

• Fixed Riot Client 26.20 compatibility
• Fixed rune provider loading
• Improved LAN reconnection

[ Update & Restart ]
```

---

## Global hotkeys

### Integrate

Official Tauri global-shortcut plugin.

### Study UX from

- RabadonGG
- TcNo Account Switcher

---

## Account shortcuts

### Integrate

Tauri deep-link support.

### Study UX from

TcNo Account Switcher.

Example:

```text
swapper://switch/<internal-account-id>
```

Never place session data or Riot credentials in shortcut arguments.

---

## Privacy mode

### Study

TcNo Account Switcher.

Implementation should remain Swapper-specific.

Privacy mode can hide:

- Riot IDs
- nicknames
- profile information
- LAN addresses
- QR pairing codes
- remote access information

---

## Account repair

No external project is necessary.

Swapper already has the required primitives:

```text
Saved account
PUUID
Encrypted session snapshot
Automatic Riot identity detection
```

Recommended repair flow:

```text
Session restore fails
        ↓
Repair Account
        ↓
Open Riot Client
        ↓
User signs in normally
        ↓
Swapper detects PUUID
        ↓
PUUID matches saved account
        ↓
Replace encrypted session snapshot
```

---

## Ready-check / champion-select notifications

### Study

- Mimic
- LeagueAkari

### Integrate

Use Swapper's existing Tauri notification plugin.

No additional library is necessary.

---

# Recommended research order

Before implementing the roadmap, inspect projects in this order:

1. **BlueBottle LeagueBroadcast Companion**
   - Closest analogue to Swapper LAN discovery/auth.

2. **LocalSend**
   - LAN discovery and polished local-network UX.

3. **Syncthing**
   - Stable device identity versus network address.

4. **KDE Connect**
   - Pair / trust / reconnect / revoke lifecycle.

5. **PairDrop**
   - Browser-based persistent pairing UX.

6. **mdns-sd**
   - Actual Rust library to integrate.

7. **Tauri plugins-workspace**
   - Updater, hotkey, deep link, notifications.

8. **Tailscale**
   - Swapper Doctor and diagnostic architecture.

9. **league-lean**
   - Provider fallback.

10. **LeagueAkari**
    - Broader League architecture and update UX.

11. **TcNo Account Switcher**
    - Privacy mode, shortcuts, protocol UX.

12. **Mimic**
    - League remote-control edge cases.

13. **LocalSend SignPath workflow**
    - Signed Windows builds.

14. **RabadonGG**
    - Update/changelog/hotkey UX only.

---

# Dependency recommendation

Do **not** turn the roadmap into dependency shopping.

Most of these repositories should influence Swapper's design, not become dependencies.

The likely new external dependencies / integrations should stay approximately:

```text
mdns-sd
Official Tauri plugins
SignPath release tooling
```

Everything else should primarily serve as architecture, implementation, or UX reference material.

This keeps Swapper small while still benefiting from mature solutions already proven by other open-source projects.
