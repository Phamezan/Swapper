# Swapper — Visual Identity Brief

## 1. Design Vision

Swapper should feel like a **premium desktop companion for League of Legends**, combining account management, champion tools, performance insights, and client utilities in one cohesive application.

The visual identity should be recognizable without relying on League artwork or the Swapper logo.

**Core identity:** Modern, refined, responsive, game-adjacent.

**Design keywords:** Graphite, restrained, crisp, confident, cohesive, tactile.

**Guiding principle:**
> Swapper should combine the polish of a premium desktop application with the personality of a gaming companion, without becoming an overly decorative esports dashboard.

## 2. Visual Direction

### Dark Graphite + Signature Green

Move away from the current blue/navy-dominant appearance toward a neutral, graphite-based interface.

- **Background:** Near-black, neutral graphite.
- **Elevated surfaces:** Slightly lighter charcoal.
- **Borders:** Thin, low-contrast neutral gray.
- **Primary accent:** Swapper's existing signature green.
- **Typography:** Off-white primary text, neutral gray secondary text.
- **Semantic colors:** Muted red, amber, and green where appropriate.

Green should function as a recognizable brand accent, not a decorative glow applied everywhere.

Avoid large blue gradients, cyan-tinted backgrounds, purple ambient lighting, or excessive neon effects. The only ambient light is a faint graphite glow with a hint of brand green in one corner (see §4a).

The interface should still feel premium when all artwork is temporarily removed.

## 3. Layout & Composition

Retain the established desktop application structure:

- Compact application title bar with navigation and search.
- Persistent sidebar navigation.
- Spacious main content region.
- Modular cards and clearly defined content groups.

Aim for **controlled information density**: enough information to make the app useful, without feeling cluttered.

### Dashboard

Keep performance statistics, ranked information, recent matches, and champion performance. These are legitimate parts of Swapper's expanded feature set.

However, avoid treating every piece of information as an equally important card.

Establish clear visual hierarchy through size, alignment, spacing, and typography.

### Account Management

Make account identity and switching particularly recognizable through profile imagery, active session indicators, and prominent primary actions.

The current active account should be identifiable at a glance.

### Champion Tools

Tier lists, builds, matchups, and champion select should share the same visual system as the dashboard and account management.

Avoid introducing a separate visual theme for each feature.

## 4. Component Language

**Geometry**
- Moderately rounded corners, approximately 8–12px.
- Consistent radii and spacing throughout the application.
- Clean rectangular surfaces rather than excessively pill-shaped components.

**Cards**
- Subtle surface separation and borders.
- Three elevation levels (card, raised/hero, popover); shadows stay soft and layered.
- No unnecessary nested containers; a card inside a card is a flat "well", never a second shadow.
- Gradients follow §4a: soft top-light fills and hairlines, artwork scrims, brand green only on accents.

**Typography**
- Modern, highly legible sans-serif.
- Strong contrast between headings, values, labels, and metadata.
- Avoid decorative gaming fonts.
- Numbers should be particularly easy to scan.

**Translucency and gradients (§4a)**

Subtle gradients and transparency are part of the premium feel, within these rules:

- *Layered translucent chrome.* Title bar, sidebar, popovers, menus, the command palette and tooltips are frosted: a translucent fill (`--ui-chrome`, `--ui-popover`) with `backdrop-filter: var(--ui-blur)` (blur + saturate) over the frame's ambient backdrop.
- *Ambient canvas.* The frame is graphite with a very soft brand-green light in the top-left corner; the page canvas is a slightly translucent inset sheet over it. No blue or purple ambient light.
- *Cards.* Semi-transparent fill that is a few percent lighter at the top (`--ui-card-fill`), a 1px inner top highlight, a hairline brighter on top than at the sides and bottom, and a layered shadow per elevation (`--ui-elev-1..3`).
- *Accents.* The brand-green gradient (`--ui-brand-gradient`) only on primary buttons, the active nav item and key highlights such as the LP bar.
- *Performance.* `backdrop-filter` only on fixed chrome and transient popovers, never on list rows, cards inside scroll containers or anything repeated. Gradients are static; nothing animates a gradient or loops. The WebView runs with `--disable-gpu`, so this is a hard rule.

Tokens live in `src/theme.css` (`--ui-*`); the shared components are listed in `src/full-app/ui/README.md`.

**Icons**
- Consistent line icon family.
- Uniform stroke weights and sizing.
- Reserve filled icons for intentional emphasis.

**Artwork**
- Champion splash art and profile icons provide personality.
- Artwork should complement the interface, not define its entire aesthetic.
- Apply consistent overlays to maintain readable foreground content.

## 5. Color Hierarchy

Follow these priorities:

1. **Neutral surfaces:** Define the overall application environment.
2. **Signature green:** Primary actions, selected navigation, focus, and brand-related emphasis.
3. **Semantic colors:** Errors, warnings, readiness, and other meaningful application states.
4. **Data visualization colors:** Distinguish data series and positive/negative trends independently of brand identity.

Do not make every chart green simply because green is the brand color.

Color must communicate meaning, not merely decoration.

## 6. Motion & Interaction

Swapper should feel quick and responsive.

- Subtle hover and focus feedback.
- Smooth navigation transitions.
- Responsive account switching interactions.
- Clear loading, pending, success, and error states.
- Restrained transitions, generally around 120–250ms.

Prioritize **interaction clarity over visual spectacle**.

Avoid excessive glow, animated gradients, perpetual movement, or unnecessarily elaborate transitions.

A successful interaction should feel deliberate and satisfying rather than flashy.

## 7. What to Avoid

- Generic dark-blue SaaS dashboard aesthetics.
- Glassmorphism beyond §4a (frosted content cards, blur on anything that scrolls) and neon gaming effects.
- Making the UI resemble a League of Legends client skin.
- Copying DPM's visual identity directly.
- Excessively dense grids of identical metric cards.
- Gradients beyond §4a: no animated gradients, no colourful backgrounds, brand green only on accents.
- Introducing additional UI libraries purely for visual decoration.
- Reworking existing functionality or navigation without a clear usability benefit.

## 8. Implementation Scope

This is a **visual identity refinement**, not a full application redesign.

Preserve existing features, information architecture, and functional behavior unless a specific change materially improves usability.

Prioritize:

1. Establishing cohesive design tokens for surfaces, typography, borders, spacing, and colors.
2. Refining shared components such as cards, buttons, navigation, and controls.
3. Applying the design system consistently to the current dashboard.
4. Extending the same language across remaining application pages.
5. Introducing subtle interaction polish where it provides meaningful feedback.

Avoid unnecessary abstractions or complex animation systems.

## 9. Success Criteria

The visual refresh succeeds when:

- Swapper no longer feels predominantly blue.
- The application has a recognizable green-led identity.
- Screens feel like parts of one cohesive product.
- Dashboard information remains easy to scan.
- Account management feels like a first-class feature.
- Gaming artwork provides character without overwhelming the UI.
- The result feels distinctive from DPM while retaining comparable polish.
- The design remains maintainable and straightforward to implement.

**Final design statement:**

> Swapper is a refined, graphite-dark League companion with a distinctive green identity, deliberate information hierarchy, and responsive desktop interactions. It balances the utility of professional software with the personality of a gaming application.
