# lopi Design Language Proposal

Research date: 2026-08-22. Scope: extend lopi's existing Konjo visual identity with the
*structural systems* (scales, adoption discipline, motion/elevation conventions) that
claude.ai has and lopi doesn't. This is not a reskin; lopi's ice/ember/jade palette stays.
No Anthropic colors, wordmarks, or logo assets are reused; values below are cited as
reference points that informed new Konjo-named tokens, not copied verbatim.

## Phase 0: Capability check

This session has two browser surfaces: an in-app Browser pane (no claude.ai session) and
Claude in Chrome (the user's logged-in Chrome, used for everything below per the user's
explicit "use chrome for Claude" instruction mid-task). All Phase 1 data below was read
via `getComputedStyle` in the page context, the programmatic equivalent of DevTools'
Computed panel, never via bundle inspection, scraping, or crawling. No login flow was
performed by the assistant; the session was already authenticated in the user's browser.

## Phase 1: Observed reference values (claude.ai, live inspection)

Every entry cites `[surface/selector, 2026-08-22]`. Values are Anthropic's; they inform
new Konjo tokens below and are **not** proposed for lopi verbatim.

### Color system
- Layered surface and text scale expressed as **HSL triplets**, not flat hex: `--bg-000`
  (white, light), `--bg-100` "0 0% 8.2353%" (dark, roughly `#151515`), `--bg-300` "0 0%
  5.098%" through `--bg-500`; `--text-000` "60 14.2857% 97.2549%" (warm near-white, 60
  degree hue) down to `--text-500` mid-grey. The hue sits at 55 to 60 degrees across the
  whole neutral ramp, not pure grey. `[:root computed styles, claude.ai/code, 2026-08-22]`
- Semantic status colors are paired text/bg/chip triples, and are reused as the *source*
  for git-status colors via `color-mix(in srgb, <hue> 20%, transparent)` rather than a
  second hardcoded set: `--cds-text-success #0ca30c`, `--cds-text-git-added #32d74b` (same
  green family, generated). `[:root, claude.ai/code, 2026-08-22]`
- Dark-to-light is a full re-declaration of the same token names (`--bg-000` flips from
  `#151515`-tier to pure white, `--cds-page-bg` flips from `#151515` to `#fcfcfb`),
  toggled via Settings > General > Appearance. `[Settings modal General pane,
  claude.ai/code#settings/general, 2026-08-22]`
  **Unobserved:** the full light-mode property set. Only `--bg-000`, `--bg-100`,
  `--cds-page-bg`, and `--cds-text-primary` were spot-checked after toggling.

### Spacing
- Two parallel scales, not one: a 4px atomic unit (`--spacing: .25rem`) driving a Tailwind
  linear scale, **and** a separate non-linear chrome-gap ladder for layout: `--cds-gap-xs
  8px`, `sm 12px`, `md 16px`, `lg 28px`, `xl 40px`. `[:root, claude.ai/code, 2026-08-22]`

### Radius
- Two ladders again: a linear display ladder (`--radius-xs .125rem` through `--radius-3xl
  1.5rem`) and a tighter compact-chrome ladder (`--cds-radius--xs 6px`, `--cds-radius--sm
  7px`, `--cds-radius 8px`, `--cds-radius--lg 10px`). One value is *derived*, not
  hardcoded: `--radius-card: calc(8px + 4px)`. `[:root, claude.ai/code, 2026-08-22]`

### Elevation
- A 3-step shadow ladder (`--shadow-sm/md/lg`) built from two stacked shadows each (a
  tight ambient shadow plus a soft diffuse one), plus a distinct `--cds-shadow-popover`
  for floating chrome and a composite `--shadow-panel` (inset 1px ring plus popover
  shadow) used for the artifact and settings surfaces. `[:root, claude.ai/code,
  2026-08-22]`

### Typography
- Three named font roles via CSS vars, not one: `--font-sans` (anthropic-sans, UI chrome),
  `--font-mono` (anthropic-mono, code), `--font-claude-response` (anthropic-serif with an
  11-deep CJK/RTL fallback chain, assistant prose body only; the user is offered
  "Anthropic Serif" as a selectable **Chat font** in Settings). `[:root and Settings >
  General > Chat font, claude.ai/code, 2026-08-22]`
- Variable-weight font axis, not fixed weights: `--cds-font-wght-regular "wght" 360`,
  medium 460, semibold 540, bold 560. This gives sub-pixel weight tuning that a fixed
  400/500/600/700 set can't. `[:root, claude.ai/code, 2026-08-22]`
- Two parallel type scales: a Tailwind display scale (`--text-xs .75rem` through
  `--text-6xl 3.75rem`, each with a paired `line-height` ratio var) and a separate compact
  **cds** UI scale for chrome (caption 12px, body 14px, heading 15px, title 22px). `[:root,
  claude.ai/code, 2026-08-22]`
- Inline `<code>` is styled with the **danger** text color, not a neutral grey. That
  distinguishes code visually from prose at a glance. `[assistant message, "Lopi component
  color theme corrections" chat, claude.ai, 2026-08-22]`

### Motion
- Base transition is `0.15s cubic-bezier(.4,0,.2,1)` (Tailwind default) for hover/focus
  color changes. A distinct, faster **snap** easing (`cubic-bezier(.32,.72,0,1)`) governs
  UI reveals, used with **asymmetric** in/out durations: message-action icons reveal in
  120ms and hide in 60ms, so hiding is deliberately faster than showing. A third
  **overshoot** curve (`cubic-bezier(.34,1.3,.64,1)`) exists for springier
  micro-interactions. `[:root and nav buttons, claude.ai, 2026-08-22]`
- A dedicated, user-facing **Motion: System / Reduced** control exists in Settings,
  separate from OS-level `prefers-reduced-motion`. The app offers its own override.
  `[Settings > General > Motion, claude.ai/code#settings/general, 2026-08-22]`
- **Unobserved:** live token-by-token streaming reveal timing (only static, already-loaded
  messages were inspected).

### Component patterns
- **Sidebar**: fixed 280px, 1px `rgba(255,255,255,.1)` right hairline, no drop shadow.
  `[<aside class="dframe-sidebar">, claude.ai/code, 2026-08-22]`
- **Composer**: 20px-radius surface, background one step up from page (roughly
  `#201F1F`), border expressed as an **inset 1px ring**, not a stroke. That avoids the
  "double border" look when nested in other rounded surfaces. `[composer wrapper,
  claude.ai/new, 2026-08-22]`
- **Session/task list row** (claude.ai/code's session list is the closest existing analog
  to lopi's own dashboard): a 5px fully-round status dot in the semantic color plus a 12px
  label in the same color ("Needs input" in amber `#db9300`/`rgb(250,178,25)`), title,
  muted subtitle, repo tag, relative timestamp, trailing chevron. One consistent row shape
  is reused for both the session list and the PR list. `[claude.ai/code home, 2026-08-22]`
- **Loading state**: full-layout skeleton blocks (grey bars sized to the eventual
  content's line lengths) for both a chat transcript load and the artifact gallery grid.
  Never a bare spinner for whole-page loads. `[claude.ai/chat/*, claude.ai/artifacts,
  2026-08-22]`
- **Settings modal**: fixed-size dialog, left category rail plus right content pane, no
  full-page navigation. `[claude.ai/code#settings, 2026-08-22]`
- **Popover/dropdown menu**: rounded card, icon+label rows, hover fill, a thin divider
  separating a destructive (red) action from the rest. `[artifact "..." menu,
  claude.ai/code/artifact/*, 2026-08-22]`
- **Artifact gallery**: card grid, thumbnail-or-icon top, title plus "Edited Nt ago"
  metadata row at bottom. `[claude.ai/artifacts, 2026-08-22]`
- **Unobserved / explicitly flagged**: the inline split pane (chat plus artifact side by
  side). Every artifact opened during this session opened full-page via the gallery, not
  from an active chat, so the open/resize/close interaction of the split view was not
  captured. Cowork's in-session UI beyond its Settings pane was not exercised (no session
  was launched). Mobile/responsive breakpoints were not tested (desktop 1314x924 only).

## Phase 2: Gap analysis vs. lopi's current state

Full inventory (file:line, spot-verified against `web/src/app.css`, `web/tailwind.config.js`,
`macos/Lopi/Theme/KonjoTheme.swift`, `web/src/lib/components/AppSidebar.svelte`) available
in this task's research trail; headline gaps below.

| Category | claude.ai has | lopi has | Gap |
|---|---|---|---|
| Color | one token source per surface, fully adopted | [KonjoTheme.swift](macos/Lopi/Theme/KonjoTheme.swift) and [app.css:5-60](web/src/app.css#L5-L60) are well-defined **and mirrored**, but ~150+ inline hex literals bypass them on web; `--konjo-panel` is referenced with a fallback in 12 files and never declared, so every consumer silently uses the fallback hex | Not a missing system, an **unenforced** one |
| Spacing | two explicit scales (atomic 4px plus non-linear chrome gaps) | no token exists on either platform: [tailwind.config.js](web/tailwind.config.js) has no `spacing` override, no Swift spacing enum | Fully missing |
| Radius | two explicit ladders, one value derived via `calc()` | no token on either platform; [KonjoTheme.swift:78](macos/Lopi/Theme/KonjoTheme.swift#L78) hardcodes `10` once, every other call site repeats its own literal | Fully missing |
| Elevation | 3-step ladder, fully adopted, one composite for floating chrome | [app.css:47-49](web/src/app.css#L47-L49) defines `--glow-sm/md` and `--shadow-pane`; **`--shadow-pane` has zero consumers, `--glow-sm` has zero consumers** | Defined, essentially unused |
| Typography | 3 named font roles, variable weight axis, 2 parallel type scales | `--font-mono` is referenced pervasively in `.svelte` files but **never declared** in `app.css`'s `:root`, so it silently degrades to generic `monospace`; no type scale exists at all (dozens of near-duplicate one-off font-size literals) | Missing scale plus a broken token |
| Motion | tokens fully adopted, dedicated user-facing Reduced-motion setting | [app.css:38-43](web/src/app.css#L38-L43) defines a clean `--dur-fast/base/slow` plus easing set, used in only about 4 of dozens of animated components; `@media (prefers-reduced-motion)` exists once globally but is re-implemented ad hoc elsewhere | Defined, roughly 90% unadopted |
| Session/task list pattern | one row shape, reused for sessions and PRs | lopi's dashboard (the actual functional analog: [lopi-ui](crates/lopi-ui)) has no single reusable row component; each surface hand-rolls its chrome | No shared primitive |
| Component reuse | `Panel`/`StatCard`-equivalent primitives adopted broadly | `Panel.svelte`, `StatCard.svelte`, `konjoSurface`, and `konjoGlow` exist but are used at 6 to 11 call sites against 100+ views; `StackCard.svelte` (2080 lines) and `StackControlDockView.swift` hand-roll their own chrome instead | Primitives exist, adoption is the gap |

**The pattern across every category**: lopi's problem is not a missing design system. It's
that lopi has *repeatedly built* partial systems (color tokens, `--glow-*`/`--shadow-pane`,
`--dur-*`/`--ease-*`, `konjoSurface`/`konjoGlow`) and then not enforced or finished adopting
them before moving to the next screen. claude.ai's advantage isn't more sophisticated
tokens; spacing, radius, and motion are simple ladders there too. The advantage is that
*nothing bypasses them.*

## Phase 3: Proposal

### New tokens

Extends the existing `--konjo-*` / `Konjo` enum naming already in both codebases; this
is completion, not a rename. Values are lopi-original (a 4px base unit and modular type
ramp are generic CSS convention, not Anthropic IP); none are copied from claude.ai.

**Spacing** (new; both platforms currently have zero):
| Token | Value | Rationale |
|---|---|---|
| `--konjo-space-1` | `4px` | Base unit. Matches the `4px`/`8px` clusters already dominant in the ad hoc data (spacing gap analysis: `4px` was already lopi's single most common literal, so this codifies existing practice rather than imposing a new one) |
| `--konjo-space-2` | `8px` | |
| `--konjo-space-3` | `12px` | |
| `--konjo-space-4` | `16px` | |
| `--konjo-space-5` | `24px` | |
| `--konjo-space-6` | `32px` | |
| `--konjo-space-7` | `48px` | Section-level gaps only |

One scale, not two. claude.ai's split (atomic plus chrome-gap) exists because Tailwind's
default scale and their hand-authored chrome scale grew independently. lopi is starting
from zero, so there's no legacy scale to reconcile; one 7-step ladder covers both needs.

**Radius**:
| Token | Value | Rationale |
|---|---|---|
| `--konjo-radius-xs` | `4px` | Chips, inline code |
| `--konjo-radius-sm` | `6px` | Buttons, inputs |
| `--konjo-radius-md` | `10px` | Panels. Codifies [KonjoTheme.swift:78,81](macos/Lopi/Theme/KonjoTheme.swift#L78)'s existing `cornerRadius: 10` literal instead of replacing it, avoiding a visual regression on `KonjoPanel` |
| `--konjo-radius-lg` | `16px` | Modals, the settings-dialog-equivalent surface lopi doesn't have yet |
| `--konjo-radius-pill` | `999px` | Status pills, tags. Already lopi's de facto convention (`StatusChip.svelte`) |

**Elevation** (replaces the two zero-consumer tokens rather than adding a third layer of
dead code):
| Token | Value | Rationale |
|---|---|---|
| `--konjo-shadow-sm` | `0 1px 2px rgba(0,0,0,.3), 0 2px 6px rgba(0,0,0,.24)` | Cards at rest |
| `--konjo-shadow-md` | `0 4px 12px rgba(0,0,0,.4), 0 1px 2px rgba(0,0,0,.3)` | Dropdowns, tooltips |
| `--konjo-shadow-lg` | `0 12px 32px rgba(0,0,0,.5), 0 0 0 1px rgba(255,255,255,.06)` | Modals, the panel-equivalent of `--shadow-pane`. This **replaces** `--shadow-pane`, it doesn't sit alongside it |
| `--konjo-glow` | `0 0 16px rgb(var(--konjo-accent-rgb) / .28)` | Single glow token, not an sm/md split. Collapses `--glow-sm`/`--glow-md` into the one variant that's actually used today (`HelpOverlay.svelte:40`) |

Two-shadow stacking (a tight ambient shadow plus a soft diffuse one) is adopted from the
*structural pattern* observed at claude.ai's `--shadow-sm/md/lg`, with lopi-original
values tuned darker to sit correctly on lopi's near-black void rather than claude.ai's
warmer dark grey.

**Typography** (the actual missing piece on both platforms):
| Token | Value | Rationale |
|---|---|---|
| `--konjo-text-xs` | `11px` | Matches the *already-dominant* ad hoc value (gap analysis: `11px` was lopi's #1 most common literal) |
| `--konjo-text-sm` | `12px` | |
| `--konjo-text-base` | `14px` | |
| `--konjo-text-md` | `16px` | |
| `--konjo-text-lg` | `20px` | Section headers |
| `--konjo-text-xl` | `28px` | Page titles. lopi's chrome-dense UI has no real "hero" text tier and shouldn't invent one |

Fix, don't extend, the broken token: declare `--font-mono` in `app.css`'s `:root` (it's
currently referenced in 8+ components and silently resolves to generic `monospace`
instead of JetBrains Mono). That's a one-line fix, not a new token.

**Motion**: lopi's `--dur-fast/base/slow` plus `--ease-*` set
([app.css:38-43](web/src/app.css#L38-L43)) already matches this proposal's bar.
**No new motion tokens are needed.** The gap is adoption (roughly 4 of dozens of
components), not design. Recommendation: replace the 5 byte-identical duplicated
`.shadow(color:.black.opacity(.6), radius:17, y:8)` Swift literals with one `Konjo`
static, and the 4 duplicated `animation: spin 1.1s linear infinite` CSS keyframe copies
with one `<Spinner>` component. That's mechanical dedup, not a design decision.

### Component pattern recommendations, mapped to lopi's actual screens

- **Session/task list row** (claude.ai/code's session list is functionally lopi's
  dashboard). Adopt claude.ai's row shape: status dot (color mapped to phase, using
  lopi's *existing* `--phase-*` tokens), title, muted subtitle, repo/branch tag, relative
  time, chevron, as **one** shared component consumed by both the web dashboard's task
  list and, structurally, the TUI's task list rendering. Today `StackCard.svelte` (2080
  lines) hand-rolls this per-instance; a shared row primitive is the single highest-value
  fix given the gap analysis's finding that `Panel`/`StatCard` exist but are barely used.
- **Tool-call accordion** (claude.ai's collapsed-by-default tool call, glyph plus name
  plus args, expand for full result) maps directly onto lopi's existing `ToolCall.svelte`.
  It already implements this pattern; the fix here is token adoption (its `0.72rem`,
  `0.7rem`, `0.6rem`, `0.65rem` one-off sizes moving to `--konjo-text-xs/sm`), not a
  redesign.
- **Loading skeletons over spinners** for whole-surface loads (dashboard first paint,
  stack list load). lopi already has one shimmer keyframe (`app.css:186,188-195`) but it
  isn't used as a full-layout skeleton anywhere observed; extend its use to the
  dashboard's initial load state instead of, or in addition to, the copy-pasted `spin`
  keyframe.
- **Settings surface**: lopi's Config tab is described (theme switcher) but nothing in the
  gap analysis suggests a unified settings *dialog* pattern (rail plus pane) exists.
  Given lopi is a developer tool with growing config surface (budget presets, model
  pinning, theme), a rail-plus-pane settings modal is worth planning for once config
  options exceed what fits one screen. Not urgent today.
- **Do not** import claude.ai's split chat-plus-artifact panel pattern. It was never
  actually observed this session (flagged above as unobserved), and lopi's TUI/web split
  isn't the same interaction shape to begin with. No recommendation without evidence.

### Non-goals

- **No Anthropic logo, wordmark, or icon-asset reuse.** lopi already has its own mark
  (the sunburst/orb motif referenced in `Konjo.swift` and `app.css`'s orb-state palette).
- **No exact palette lift.** Every hex value above is lopi-original, chosen to sit
  correctly against lopi's existing near-black void and ice/ember/jade spectrum, not
  copied from claude.ai's `--bg-*`/`--text-*` HSL values.
- **No claim of official Anthropic affiliation.** This document exists to make lopi's own
  Konjo identity more *consistent*, not to make lopi resemble claude.ai. Claude-branded UI
  (`anthropic-sans`, the sunburst wordmark icon) belongs to Anthropic; none of it is
  proposed for lopi.
- **No SwiftUI/web pixel parity assumption.** [KonjoTheme.swift](macos/Lopi/Theme/KonjoTheme.swift)
  already diverges from web deliberately (`bg1`/`bg2` surface tiers have no CSS-var
  equivalent). The token *names* should match across platforms so a designer can reason
  about them once, but native platform conventions (SF Symbols, macOS window chrome,
  `.regularMaterial`) should win over forcing web's exact visual output onto native.
- **No new motion-token system.** Established above: lopi's motion tokens are already
  fine; this proposal touches adoption only.
- **No redesign of the tool-call accordion, transcript block model, or phase-color
  semantics.** These already work; only their token *sourcing* changes.

### Phased adoption plan

**Phase A, web token completion (1 sprint).** Fix the two broken tokens (`--font-mono`
undeclared, `--konjo-panel` undeclared). These are correctness bugs, not design work, and
should land regardless of the rest of this proposal. Add the spacing/radius/type-scale
tokens to `app.css`. Do **not** mass-replace existing literals yet; land new components
against the new tokens and leave legacy files alone. *Stop condition: if adding the token
declarations causes any visible regression (they're purely additive `:root` vars, so this
should be near-zero-risk), revert and re-scope before touching Phase B.*

**Phase B, web adoption on the highest-leverage surface (1 to 2 sprints).** Extract a
shared session/task-list-row component from `StackCard.svelte`'s hand-rolled chrome,
consuming the new tokens. This is the single change the gap analysis flags as highest
value (dashboard is lopi's most-used, most duplicated surface). *Stop condition: if
`StackCard.svelte`'s 2080 lines can't be safely decomposed without a full rewrite in one
sprint, ship the row component for **new** dashboard surfaces only and leave `StackCard`
as legacy. Do not force a risky mid-sprint rewrite to hit full adoption.*

**Phase C, native mirroring the token names (1 sprint, after B ships and holds).** Add
`Konjo.space`/`Konjo.radius` static enums to `KonjoTheme.swift` mirroring Phase A's names,
not values (native keeps its own tuned numbers per the non-goals above). Replace the 5
duplicated `.shadow()` literals and centralize the spin/loading pattern. *Stop condition:
web adoption (Phase B) should be proven out before native follows the same naming. If
Phase B's row-component pattern needed significant rework post-ship, revise the token
names before propagating them to Swift, not after.*

**Explicitly deferred, no phase assigned:** the settings rail-plus-pane pattern (no urgent
driver yet), any split-panel/artifact-adjacent pattern (insufficient observation this
session), typography weight-axis tuning (lopi's native fonts don't currently need
variable-weight support; SF system fonts already cover the native side, and while web's
Inter variable font *does* support it, that isn't in scope until the type-scale tokens
above are adopted first).
