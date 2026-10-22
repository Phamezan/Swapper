# Swapper: visual identity

How the Fullsized App looks and why. Tokens: `src/theme.css` (`--ui-*`). Shared pieces: `src/full-app/ui/README.md`.
The design direction came from the owner's wireframes (Wireframer, project `swapper-premium`); where a wireframe and the
app's real behaviour disagree, the real behaviour wins and is restyled.

**One line:** a graphite League companion with one green accent, quiet type, and champion art for ambient colour.

## 1. Principles

- **Graphite, not blue.** Neutral near-black surfaces in four layers (frame, canvas, card, raised). No blue, purple or neon ambient light.
- **Swapper green `#c5e39e` is the brand and the action accent.** Primary actions, selection, focus, the active account, the LP bar. It is not a decoration and not a chart colour.
- **Champion art supplies ambient colour.** The most played champion's splash fills the top of every page canvas, faded left and down so it never competes with content. Without art the page is still finished.
- **Colour means something.** Win/loss, tiers, ranks and data keep their own colours; warnings, errors and info use the three alert tones below.
- **Controlled density.** Hierarchy comes from size, alignment and spacing, not from putting every figure in its own card.

## 2. Type rules

1. **Bold uppercase is for real actions only** (Switch account, Open Riot Client, Save account, Apply all, Start League, Replace). It is 12.5px, weight 750, `.07em` tracking. Everything else is quiet sentence case: filters, tabs, dropdowns, section labels, secondary links, table headers. **No ALL-CAPS eyebrow labels.**
2. **Readable floor.** 13–14px for useful supporting information, 12px only for genuinely secondary metadata (timestamps, counts, kbd hints). Nothing under 12px, including decorative separators.
3. Outfit throughout. Page title 24px/750, section title 16px/650, body 14.5px, label 13px, caption 12px. Numbers use tabular figures.

Tokens: `--ui-title 20`, `--ui-body 14.5`, `--ui-label 13`, `--ui-caption 12`.

## 3. Surfaces and elevation

- **Frame** `--ui-surface-0` carries a faint brand-green light in its top-left corner. **Canvas** is an inset sheet over it. **Cards** (`--ui-card-fill`) are a few percent lighter at the top, with a brighter top hairline and a 1px inner highlight.
- Elevation 1 cards, 2 raised cards and heroes, 3 popovers and menus. A card inside a card is a flat well, never a second shadow.
- Radii 6/8/12/16 (`--ui-radius-*`). Lines `--ui-line-1..3`.
- **Translucency, with limits.** Title bar, sidebar, account picker, menus, the Ctrl+K palette and tooltips are frosted (`--ui-chrome`, `--ui-popover`, `--ui-blur`). `backdrop-filter` is never used on anything that scrolls or repeats. No animated gradients; the only looping motion is the indeterminate bar of the switching card, which stops under `prefers-reduced-motion`. The window runs with `--disable-gpu`, so paint stays static.

## 4. Banners and alerts

Neutral glass surface (`--ui-card`), a 3px left edge in the tone, a faint glow from the left and an icon tile in the tone. **Never a tinted fill**: a low-alpha amber over graphite turns muddy brown.

| Tone | Colour | Token |
|---|---|---|
| Warning | `#ffbe55` | `--ui-warn` |
| Error | `#ff7d76` | `--ui-error` |
| Info | `#8fd0e6` | `--ui-info` |

`Notice` (ui components) and the hosted flyout banners (`error-banner`, `reconnect-banner`, `pairing-notice`) share this recipe.

## 5. The shell

- **Title bar:** the Swapper icon and a prominent wordmark form a brand section in the top-left corner, above the sidebar and outside the profile artwork. Back/forward/refresh, search (`Ctrl+K`) and window buttons follow. Translucent chrome.
- **Sidebar:** an integrated profile at the top, with the portrait centered and the player's name and tag together on one centered line. Rank and LP sit below. The profile has no separate outer card border. A distinct **Switch account** button sits at its foot; only that button opens the account picker. The flat navigation list follows, with groups separated by a hairline and spacing. The active page has a green tint and a thin green border.
- **No status footer.** The podium shows who is signed in. A **dot on the portrait** shows the client is connected (green) or not (amber). A **one-line sidebar status** appears only for a problem ("League Client isn't connected. Showing saved stats.", "Open League Client to see your stats.") or something in progress ("Switching to X…"), with an amber or green left edge.
- **Account picker:** a popover anchored directly to **Switch account**, with one flat account list and no subsection headings. Each row shows a portrait, name, tag, region and rank; the current row has a check. A footer links to **Manage accounts**, where accounts can be added. Long lists have a filter and scroll within the available window height. Esc returns focus to the selector; an outside click closes the popover. It calls the same switch path as the Accounts page.
- **Switching accounts:** the page dims and one card names the target account. The switch flow reports only started, done and failed, so the progress bar is honestly indeterminate.
- **Ambient art** sits on the canvas background itself (static, so scrolling never repaints it).

## 6. Everyday states

Designed with the same care as the full pages:

- **Loading:** `Skeleton` rows shaped like the content, static.
- **Empty:** `EmptyState`, a dashed card with an icon, a title and what to do next.
- **Client disconnected:** amber portrait dot plus the sidebar status line; stale pages keep their saved data and say so.
- **Switching accounts:** the dimmed page and the switching card.
- **Failed actions:** an error `Notice` or flyout banner with the reason and, where one exists, the action (Repair account).

## 7. Pages

- **Accounts:** three-up cards (identity, region, session, the Switch account action); the active card has a green edge and a quiet disabled "Currently signed in". "Add another account" is a compact dashed row. The add, edit, remove and repair flows are a centered card with the same type rules.
- **Settings:** a quiet tab list, then settings rows in a card. Overlay settings show widget previews; the **Skill level-up** preview is drawn as the game draws it: a graphite box with a green double chevron and three green diamonds above, not Q W E R keys.
- **Live Game:** two team cards, one row per player; items are always six slots and the trinket as framed slots (empty slots are framed, not blank); the queue and clock sit in a green status chip.
- **Items in match views:** one row, 6 slots + trinket, fixed slots, framed empty slots (`Slot`).

## 8. Motion

Quick and quiet: 120–180ms colour and border transitions, a 2px lift on clickable cards, no spectacle. Reduced motion removes lifts and the switching bar's movement.

## 9. What to avoid

- Blue or purple ambient light, neon glows, glass on scrolling content.
- ALL-CAPS labels that are not actions; text under 12px.
- Tinted alert fills.
- Making the UI a League client skin or copying another stats app's identity.
- New UI libraries for decoration, or animated gradients.

## 10. Where it lives

- Tokens and type scale: `src/theme.css`. Mappings from older names (`--fa-*`, shadcn): top of `src/full-app/full-app.css`.
- Shell components: `src/full-app/ui/Podium.tsx` (podium, account picker, switching overlay) and `shell.css`.
- Shared components: `PageHeader`, `Slot`, `Skeleton`, `EmptyState`, `Notice` in `src/full-app/ui/`.
- The "Revision 3" blocks at the end of `full-app.css` restyle the hosted flyout pages (banners, settings, account sub-pages) without touching the flyout itself, whose look is unchanged.
