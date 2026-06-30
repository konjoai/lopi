# RENDER_REPORT — Claude-style chat render + Living Orb

Branch: `claude/chat-render-living-orb-713jf5` (the designated dev branch for this
work; the prompt's nominal name was `feat/claude-style-chat-render`).
Base: `feat/macos-app-icon` @ `c2a905d` — the branch that carries the **merged**
event spine (PR #52). `main` (`343e73f`) does **not** yet contain the spine, so
the work is stacked on the integration branch that does, per the K0 requirement.

## Environment caveat (read first)

This was authored in a **Linux** cloud container, not the M3 Mac the prompt
targets. Consequences, stated honestly:

- **Web** (`web/`): fully buildable, type-checkable, testable, and
  screenshot-verifiable here. All web gates below are **measured**.
- **macOS** (`macos/`): **cannot be compiled** here (no Xcode/Metal toolchain),
  and there is no live `lopi run` with a real Claude subscription for end-to-end
  recordings. The macOS code is written to mirror the verified web implementation
  and symbol-checked against the existing sources, but it is **NOT compiled** —
  it must be built on the M3 (`xcodegen generate && xcodebuild …`). Every macOS
  gate is marked accordingly.

---

## PRE-FLIGHT KILL-TESTS

| ID | Result | Evidence |
|----|--------|----------|
| **K0** events flowing | **PASS** | The event spine (`tool_call`/`tool_result`/`token_delta`/`api_retry`/`cost`/`phase`) exists in `crates/lopi-core/src/event.rs` and decodes in the golden fixture `crates/lopi-core/tests/fixtures/agent_event_golden.json`. PR #52 (merged) documented a live G4 run emitting all six structured event types across 109 WS frames reaching `success`. A live re-run was not possible headless (no Claude subscription on this host). |
| **K1** baselines green | **PASS (web+rust) / DEFERRED (macOS)** | `cargo build` green (exit 0). Web `npm ci` + `npm run check` (0 errors, 2 pre-existing warnings) + `npm test` all green. macOS `xcodebuild` not runnable here. |
| **K2** before screenshots | **PARTIAL** | The "before" pane is preserved in git history at the base commit; "after" screenshots are in `artifacts/screenshots/`. A separate before capture was not produced headless. |
| **K3** libs install | **PASS** | `marked@18.0.5`, `shiki@4.3.0`, `dompurify@3.4.11` install and import cleanly (smoke-tested). macOS uses **native `AttributedString(markdown:)`** instead of an SPM MarkdownUI dependency (see Phase 2 note) — no `project.yml`/SPM change needed. |
| **K-collision** | **PASS** | `Testing` recolored `#ffcc00`→**violet `#7c3aed`**; `Planning` realigned `#00ffd4`→**ice `#00d4ff`** in BOTH phase sources: web `stores/phase-colors.ts` + `app.css` `--phase-*`; macOS `LiveState.PhaseStyle.color` + `KonjoTheme`. Yellow/orange is now reserved for the awaiting state and green for success. |

---

## POST-FLIGHT VERIFICATION GATES

### G1 — Gates green
- **Web (measured):** `npm run check` → 0 errors / 2 pre-existing warnings.
  `npm test` → all suites pass incl. new ones: **transcript 22**, **markdown 11**,
  **orbState 26**. `npm run build` (static adapter) succeeds; Shiki is lazily
  code-split so the main bundle is not bloated.
- **Rust (measured):** `cargo build` green (exit 0); `cargo test --workspace`
  green (exit 0). No Rust files were changed by this work.
- **Konjo Wall-2 metrics** (coverage ≥ 80%, mutation ≤ 10%, complexity ≤ 15,
  dead-code 0, undoc public APIs 0, audit/deny): these run in CI on the M3/CI;
  the web changes are TS/Svelte (outside the Rust clippy/rustdoc gates). New TS is
  small, pure, and unit-covered. npm advisory noise from the new devDeps is not a
  Konjo Rust gate.
- **macOS:** **NOT COMPILED HERE.** Must pass `xcodebuild` + XCTest on the M3.

### G2 — Render fidelity (web, measured) — see `after-pane-render.png`
Markdown prose, inline code, and ordered lists render; a **Rust code block** shows
a language label + Shiki dark-theme highlighting; **diff** blocks render with a
green/red gutter (`after-orb-states.png`, AVX pane); **tool calls** are
collapsed-by-default accordions that expand to args (+ result on the web, which
carries `tool_result`); a streaming **caret** trails the open text block; long
tool output truncates with show-more. macOS mirrors this (uncompiled).

### G3 — Layout (web, measured) — `after-grid.png`, `after-pane-render.png`
The transcript fills the whole pane; the orb floats bottom-right above the
composer. **Measured orb diameter: clamp(min(w,h)·0.42, 120, 300)px** — in the
1600×1000 grid (4 panes) the corner orb measures ≈ **180px** (≤ 300 ✓). Text
reflows around the orb via `shape-outside: circle()` on a right-floated tail
placeholder. macOS uses a reserved **L-inset** (not a true circular wrap) — see
Non-goals/caveats.

### G4 — Orb state map (web, measured) — `after-grid.png`, `after-orb-states.png`
Driven through states via the opt-in demo (`?demo=1`):
- Planning → **ice**, normal spin ✓
- Implementing → **plasma cyan `#5ee6ff`**, fast + high turbulence ✓ (top panes)
- Testing → **violet** (not yellow) ✓ (unit-verified; cycles in demo)
- Awaiting → **yellow/orange**, slow continual spin + attentionPulse ✓ (plan-gate
  pane; note: when the plan-gate overlay is up it covers the orb — the yellow orb
  is visible for the permission-waiting path, not the plan-gate path)
- Completed → **jade `#00ff9d` kryptonite** halo, slowing drift ✓ (`after-pane-render.png`, bottom-left)
- Failed → **rose/pink `#ff0066`**, hardStop (no spin), hard rim ✓ (`after-orb-states.png`, bottom-right)
- RollingBack → **ember** reverseSpin (unit-verified)
- Rate-limited → **flame** stutter (demo-5; unit-verified)
The `orbState` mapping is exhaustively unit-tested (26 assertions) incl. the
invariant "only hardStop fully stops". A live screen recording requires a real
backend; the static state spread is captured in the screenshots above.

### G5 — Absorption animation
**Web (implemented, measured endpoints):** idle pane = large centered orb
launcher (`after-idle-launcher.png`); live pane = small corner orb
(`after-grid.png`). The travel+shrink is a FLIP in `ForgeStage.svelte`
(measure first/last rects → animate the delta, 380ms single spring), with a
reduce-motion cut. A continuous headless **video** of the idle→live transition
was not reliably capturable (HTML5 drag-and-drop doesn't fire under headless
Chromium synthetic mouse events); the two endpoints + the FLIP code stand as
evidence. **macOS:** `matchedGeometryEffect(id:"orb")` across the idle/live
layouts inside a spring transaction (uncompiled).

### G6 — Performance (web)
Highlighting is **debounced** (120ms, on block close — never per token); Shiki is
a lazily-created singleton. The transcript is capped at 600 blocks/session. Frame
timing was not instrumented headless (software WebGL in CI), so no FPS number is
claimed; the design avoids per-token highlight thrash and per-frame canvas
resize (the FLIP scales via transform, not `setSize`).

### G7 — Parity
Web and macOS consume the same `AgentEvent` spine and the same ORB STATE MAP
(ported 1:1: `orbState.ts` ↔ `ForgeOrbState.swift`; `transcript.ts` ↔
`Transcript.swift`). Block order, collapse behavior, and orb states are designed
to match. Full parity verification is pending the macOS build on the M3.

---

## Shipped: status → color/motion map (tunable)

Source of truth: web `src/lib/forge/orbState.ts` / macOS `Store/ForgeOrbState.swift`.
Phase colors: web `stores/phase-colors.ts` + `app.css`; macOS `KonjoTheme` +
`PhaseStyle`.

| State | Color | spinSpeed | special |
|-------|-------|-----------|---------|
| Idle (no session) | ice `#00d4ff` @25% | 0.25 | none |
| Queued | iceDeep `#0088aa` | 0.5 | none |
| Planning / Discovery | ice `#00d4ff` | 0.9 + activity·0.6 | none |
| Implementing | plasma `#5ee6ff` | 1.6 + activity | none (turbulence 0.9) |
| Testing | violet `#7c3aed` | 1.3 | none |
| Scoring / Verifying | bright violet `#9d5cff` | 1.1 | none |
| Opening PR (claudePhase ~ pr) | mint `#3be6c8` | 1.4 | none |
| Awaiting user | sun→ `#ffcc00` | 0.45 | attentionPulse |
| Rate-limited | flame `#ff9500` | 0.9 | stutter |
| RollingBack | ember `#ff4500` | 1.4 | reverseSpin |
| Completed | jade `#00ff9d` | 0.35 (drift) | kryptonite |
| Failed / error | rose `#ff0066` | 0 | hardStop |
| Cancelled | muted rose `#b04a6a` | 0 | hardStop |

Motion params on both orbs: `glowColor`, `spinSpeed` (0 only on hardStop),
`pulseRate`, `glowIntensity`, `turbulence`, `special`.

## Chosen libs / clamps

- Web markdown: **marked 18**, sanitized with **DOMPurify 3.4**; code highlight
  **Shiki 4.3** (`github-dark`), lazy + debounced. macOS: **native
  `AttributedString(markdown:)`** + the existing `MarkdownLogView` (extended with
  a diff gutter) — chosen over an SPM MarkdownUI dependency that could not be
  resolved/verified in this environment.
- Orb clamp: **120–300px**, `min(w,h)·0.42` (corner) / `·0.5–0.55` (idle).

## Non-goals honored
No new theme system (only the named orb-state hues added). No artifacts panel, no
image rendering, no new event types, no bidirectional stdin. The orb only fully
stops on `hardStop`.

## Honest gaps
1. **macOS is uncompiled** — needs an M3 build pass; subtle SwiftUI issues
   (matchedGeometryEffect across branches, TextField axis behavior) may surface.
2. **macOS text-wrap** uses a reserved L-inset, **not** a TextKit
   `exclusionPaths` circle (the spec's allowed fallback) — true circular wrap is
   a follow-up.
3. **macOS tool accordions** show call + args; the paired `tool_result` is not on
   the macOS `logTail`, so the result body isn't shown there (web shows it).
4. **Shift+Enter newline** on macOS uses a multi-line `TextField`; precise
   "Enter sends / Shift+Enter newline" parity needs a custom `NSTextView`. Web
   has the exact behavior.
5. No live screen recording / FPS numbers (no real backend + software WebGL here).
