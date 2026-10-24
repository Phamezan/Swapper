# Swapper icons

Swapper's own icon set. `import { SearchIcon } from "./ui/icons"`; each icon is a named export (tree-shaken).
Props: `size` (px, default 24), `className`, plus any SVG prop (`strokeWidth`, `aria-label`…). Icons are
`aria-hidden` by default; a standalone icon button keeps its `aria-label` on the button.
Contact sheet (dev only): `/src/full-app/ui/icons/icons-preview.html`.

## Grid

- 24px viewBox, live area 3–21, stroke 1.75, round caps and joins (matches Outfit), corners rx 1.25–3.
- Colour is `currentColor`. In a nav item with `aria-current="page"` (or with class `is-active`) the icon turns
  brand green and its `ic-duo` shape fills at 22% (`icons.css`).
- Motifs: the brand mark's half-barb arrows (Switch, Refresh), the hexagon (Settings, Error), card slots (Draft, Tierlist).
- Anything not listed here stays on lucide (window controls, close, chevrons in lists, loaders, item-path arrows).

## Icons

| Icon | Meaning | Used in |
|---|---|---|
| `DashboardIcon` | Overview tiles | Sidebar nav |
| `TierlistIcon` | Ranked rows with tier badges | Sidebar nav |
| `MatchupsIcon` | Crossed blades | Sidebar nav, Dashboard empty match history |
| `DraftIcon` | Two pick cards, front one locked in | Sidebar nav, Draft waiting emblem, empty draft slots |
| `LiveGameIcon` | Map frame with live signal | Sidebar nav |
| `AccountsIcon` | Two profiles | Sidebar nav |
| `SettingsIcon` | Hex nut | Sidebar nav, tray flyout header |
| `SwitchIcon` | The brand's two half-barb arrows | Podium "Switch account", account card switch button |
| `AddAccountIcon` | Profile with plus | Account picker, Accounts "Add another account", tray "Add account" |
| `AddIcon` | Plus | Tray empty-state symbol |
| `SearchIcon` | Lens | Title bar search and palette, champion/opponent/picker searches |
| `BackIcon` / `ForwardIcon` | Chevrons | Title bar history, matchup strip scroll, tray back button |
| `RefreshIcon` | Arc with half barb | Title bar refresh stats, tray update/try again |
| `PlayIcon` | Rounded triangle | Sidebar PLAY button (opens League when it is closed) |
| `CheckIcon` | Apply / saved | Build "Apply all" and applied chips, tray save and active marker |
| `WrenchIcon` | Repair / bugfix | Patch notes bugfix lines without a champion name |
| `InfoIcon` | Information | `Notice tone="info"`, tray browser-preview banner |
| `WarningIcon` | Warning | `Notice tone="warn"`, tray reconnect banner |
| `ErrorIcon` | Error (hexagon) | `Notice tone="error"`, tray error banner |
| `SuccessIcon` | Done | Not used yet; for success notices |
