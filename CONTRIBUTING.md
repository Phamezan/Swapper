# Contributing

## Development

Requires Windows, Node.js, Rust with the MSVC toolchain, and the [Tauri 2 Windows prerequisites](https://v2.tauri.app/start/prerequisites/).

```powershell
npm ci
npm run tauri dev
```

`npm run build` checks and bundles the frontend. `node --test src/full-app/*.test.mjs src/onboarding/*.test.mjs` runs the frontend unit tests and `npm run test:e2e` runs the end-to-end checks against the preview pages with mocked data. `node tests/perf/scroll.mjs` measures scroll frame times. Set `SWAPPER_DATA_DIR` to keep a test build's own data out of `%LOCALAPPDATA%\Swapper`; Riot paths are unaffected. See [docs/RELEASING.md](docs/RELEASING.md) for publishing a release. `cargo test --manifest-path src-tauri/Cargo.toml --locked` runs native tests. To make a Windows installer locally, run `npm run tauri -- build`; a release also needs a manual test with Riot Client and multiple accounts.

During `npm run tauri dev`, the phone page is proxied to the Vite dev server instead of being served from the bundled `dist/` assets, so it stays in step with the desktop UI on every reload. Release builds serve the bundled assets as usual.

To refresh the offline matchup guides before building, run `npm run guides:sync -- "C:\path\to\swapper-guides\dist"`. This copies hash-verified compiled files into `src-tauri/resources/guides`; the checked-in snapshot starts empty. Runtime downloads live under `%LOCALAPPDATA%\Swapper\guides-cache` and refresh at most every six hours, falling back to the last good cache and then the bundle.

Matchup records stay in `%LOCALAPPDATA%\Swapper\matchup-records.json` (or under `SWAPPER_DATA_DIR`). Swapper imports each signed-in saved account's reachable history and then new games, combining draft queues 400/420/440 across saved accounts. The client can ignore pagination and repeat only its latest 20 games; older locally recorded games are retained. Remakes and ambiguous lane opponents are skipped. Outdated tip flags use positive champion changes from the available current/previous patch notes; cooldown tokens always use live data.

## Contributing

Bug reports, documentation fixes, and code contributions are welcome. For a bug, open a GitHub issue with the Swapper version, Windows version, steps to reproduce, and what happened. Remove Riot account identifiers, session files, access tokens, and other private data from screenshots and logs before sharing them.

For code changes, open an issue first if the change is substantial, then work on a focused branch and submit a pull request. Explain the behavior you changed and how you tested it. Run `npm run build` and `cargo test --manifest-path src-tauri/Cargo.toml --locked` locally; for Riot Client, League Client, account switching, or phone remote changes, include the manual scenarios you tested. Automated GitHub checks may not run on every pull request, so include your local results.
