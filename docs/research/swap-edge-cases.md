# Account swap: edge cases to test

Scope: `start_switch` (src-tauri/src/commands/accounts.rs) → `riot::switch_account` (src-tauri/src/riot.rs), its entry points, and what runs after it (League launch watch, session watch, notifications, UI events).

Entry points: flyout account row, full app Accounts page, tray menu, global hotkey (hotkey.rs), desktop/deep-link shortcuts (shortcuts.rs), Restart client (client_restart.rs), phone remote, add/repair flows that end in a switch (complete_add).

Legend: **[mock]** testable with the existing browser e2e harness (tests/e2e, mocked Tauri commands); **[unit]** pure Rust logic; **[live]** needs a real Riot Client and real saved accounts.

## 1. Preconditions and guards
1. Switch while another switch runs (double-click, hotkey + click, phone + PC at once) → second gets "Something else is running" and the first finishes. [unit][mock]
2. Switch to the account that is already active (restart same account path, `active_id == id` re-read). [live]
3. Switch with no active account but a live Riot session exists → must refuse ("Save your current sign-in first"). [unit][live]
4. Target account removed between click and run (removed in other window/phone). [unit][mock]
5. Target vault snapshot missing/corrupt/unreadable (DPAPI fail, file deleted) → fails before stopping the client. [unit][live]
6. Game (`League of Legends.exe`) running → refused, nothing closed. [live]
7. Process table unreadable → refused, not guessed. [unit]
8. Riot Client path unknown/moved/uninstalled (non-Deceive) → LauncherMissing before anything is stopped. [live]
9. Deceive on but Deceive binary missing/quarantined by antivirus. [live]
10. Live identity is a different account than `active_id` (user logged in manually in Riot Client) → "A different account is signed in. Nothing was changed." No snapshot overwritten. [unit][live]
11. Identity detection fails (LCU down, client mid-start, client on login screen, offline) → falls back to offline subject check; never writes an unverified snapshot. [unit][live]
12. Account saved without a PUUID (old config) → no session watch; is verify skipped safely? [unit]

## 2. Stopping the clients
13. Riot Client open at login screen / signed in / in patching / in a queue / in champ select / in end-of-game lobby / in post-game honor screen.
14. Riot Client minimized to tray only (no window).
15. Client refuses to close (hung, admin-elevated process while Swapper is not elevated) → ClientStillRunning, live session untouched.
16. Game starts between `ensure_no_game` and `stop_clients` (queue pop accepted) → ForbiddenPresent, nothing killed.
17. Riot Client auto-updating its own binaries while being killed.
18. Vanguard (vgc) running / requiring reboot after switch.
19. Other Riot games open (Valorant, LoR, 2XKO) sharing Riot Client.

## 3. Capture / restore / save (data-loss paths)
20. Capture of the outgoing session fails (file locked, disk full) → abort, nothing restored over it.
21. Restore of the target fails mid-write → previous session restored, or live cleared when there was none.
22. `vault::save(config)` fails after restore (disk full, AV lock) → rollback of active_id and live session.
23. Swapper killed/crashes/PC sleeps between restore and save (half-switched state on next start).
24. Outgoing session token refreshed by Riot since last save (stale snapshot) → is the fresh one captured and saved to the right account?
25. Two accounts with the same display name/nickname; account renamed in Riot (Riot ID change) → identity by PUUID only.
26. Saved session expired / "Stay signed in" not checked on the target → switch "succeeds" but Riot shows login; session watch should flag it and offer Repair.
27. Riot forces re-login on all devices (password change, security reset) for the target.
28. Region / shard differences (account on another region; KR/CN clients).

## 4. Launch and after
29. Launch fails (Riot exe gone, permission denied) → rollback restores previous session and active_id.
30. Riot Client opens but needs an update / EULA / 2FA / captcha → `launch_league_when_signed_in` times out with "League didn't start…" banner + Open Riot Client.
31. User cancels or starts another switch while launch watch waits → epoch cancels the old watch, no stale banner.
32. League auto-launch while user closed Riot Client manually during the wait.
33. Deceive mode: Deceive's own prompts/tray; status offline actually applied; switching Deceive on/off between switches (DeceiveRestartNotice).
34. Session watch: dead-on-arrival session detected → repair prompt; watch must stop when another action starts.
35. Notifications: success / failure toasts with notifications disabled; tray tooltip resets after both outcomes (including the join-error path).
36. UI events: `switch_started` / `switch_done` / `switch_failed` / `launch_failed` reach both flyout and full app; busy spinner always clears; error banner auto-dismiss (5 s) doesn't hide the Repair/Open Riot Client action while hovered.

## 5. Entry-point specific
37. Hotkey pressed while a game is focused/fullscreen (refused, user still informed?).
38. Shortcut/deep link for an account that no longer exists or with a malformed id.
39. Phone remote switch while PC is locked / asleep / Swapper window hidden; phone sees final result.
40. Restart client (same account) during champ select → plan "ask"/"defer" messages.
41. Switch fired at startup via autostart before AppState / tray exists.
42. Tray menu Switch while the full app has unsaved forms (add/edit account open).

## 6. Environment
43. Swapper not elevated vs Riot Client elevated (and vice versa).
44. Non-default Riot install path, path with unicode/spaces, multiple Riot installs.
45. Low disk space; antivirus locking the vault folder; OneDrive-redirected AppData.
46. Multiple Windows users / fast user switching (another user's Riot Client running).
47. Clock skew (token expiry checks) and offline network during switch.
48. Many accounts (20+): list order, search, hotkey to the right one.
