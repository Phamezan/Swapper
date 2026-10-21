# Fullsized App UI foundation

Shared pieces for every Fullsized App page. Tokens are `--ui-*` in `src/theme.css`;
the older `--fa-*`, `--surface-*`, `--text-*`, `--tier-*` and shadcn names are aliases
of them (top of `full-app.css`), so old code keeps working. New code reads `--ui-*`.
Rules for translucency and gradients: `docs/visual-identity.md` §4a.

## Components

| Component | File | Props | Purpose |
|---|---|---|---|
| `PageHeader` | `PageHeader.tsx` | `title`, `crumbs?: { label, onClick? }[]`, `actions?` | One-row page header (~40px): breadcrumb trail, title, right-side actions. Every page uses it. |
| `Slot` | `Slot.tsx` | `kind: "item" \| "spell" \| "rune" \| "champion"`, `id`, `size?` (px), `label?` (tooltip), `pending?`, `className?` | Framed icon square (round for runes; add `is-trinket` for a round item). `id <= 0` draws a subtle empty slot. `pending` draws the frame without loading the icon (lazy rows). Omit `size` to size it from page CSS via `--slot`. |
| `Skeleton` | `States.tsx` | `label` (screen reader), `rows?`, `height?` | Static placeholder rows for loading states. No shimmer. |
| `EmptyState` | `States.tsx` | `title`, `icon?`, `children?` (text), `action?` | Centered "nothing here yet" message for a page or card. |
| `Notice` | `States.tsx` | `tone?: "info" \| "warn" \| "error"`, `children`, `action?` | Inline notice row with icon. `error` is `role="alert"`, the rest `role="status"`. |
| `fmtCount`, `fmtCompact` | `format.ts` | `value: number` | Numbers: counts as `183,275` (en-US on every locale), large stats as `22.4k` / `1.2M`. |

## Classes (`ui.css`)

- `ui-card`, `ui-card is-raised`: elevation 1 and 2 card surfaces. `ui-well`: flat inset surface for a card inside a card.
- `ui-section-head`: section title (16px) with an optional `<small>` caption on the right.
- `ui-row`: clickable row (hover and press tint, focus ring). `ui-lift`: clickable card (lifts 2px, elevation 2 on hover).
- Existing `.fa-card`, `.panel`, `.tier-table-wrap`, `.md-team`, settings groups, overlay cards and the stat strip already get the elevation-1 card look from `full-app.css`.

## Key tokens

- Surfaces `--ui-surface-0..3` (frame, canvas, card, raised) + `--ui-surface-hover`; translucent `--ui-chrome`, `--ui-popover`, `--ui-card`, `--ui-scrim`, `--ui-well`.
- Lines `--ui-line-1..3`, `--ui-highlight`. Text `--ui-text-1..3` (primary, secondary, muted).
- Brand `--ui-brand`, `--ui-brand-strong`, `--ui-brand-ink`, `--ui-brand-tint`, `--ui-brand-line`, `--ui-brand-gradient(-hover)`.
- Data `--ui-win`, `--ui-loss`, `--ui-warn`, `--ui-gold`, `--ui-champ`, tiers `--ui-tier-splus..d`.
- Radii `--ui-radius-xs/sm/md/lg` (6/8/12/16). Elevation `--ui-elev-1..3`, `--ui-inner-hi`. Fills `--ui-card-fill`.
- Blur `--ui-blur` (fixed chrome and popovers only). Motion `--ui-fast` 120ms, `--ui-base` 180ms, `--ui-ease`; `--ui-hover`, `--ui-press`.
- Type `--ui-display` 30, `--ui-title` 20, `--ui-body` 14.5, `--ui-label` 13, `--ui-caption` 12 (minimum for readable text). Stats use tabular numerals (set on the frame).
