# macOS Validation Handoff — Claude-style chat render + Living Orb

**Run this on the M3.** Paste the "PROMPT FOR CLAUDE CODE" block below into Claude
Code from the repo root, or just open this file and say "execute this handoff."

It validates the macOS half of PR #53 (`claude/chat-render-living-orb-713jf5`),
which was authored on a Linux host and therefore **never compiled**. The web half
is already verified (see `artifacts/RENDER_REPORT.md`). Your job is G1–G7 for
macOS: make it build, run it, and confirm it matches the web behavior.

---

## Context (what this PR does)

Turns the agent pane into a Claude.ai-style chat (markdown, syntax-highlighted
code, collapsible tool-call accordions, streaming caret) and absorbs the Forge
orb into the bottom-right corner as a small **living status indicator** whose
color + motion encode the agent's status. Built on the merged event spine
(`tool_call`/`tool_result`/`token_delta`/`phase`/`cost`). Full spec + the
shipped status→color/motion map is in `artifacts/RENDER_REPORT.md`.

## macOS files to validate (the entire macOS delta)

| File | Change | Risk |
|------|--------|------|
| `macos/Lopi/Store/ForgeOrbState.swift` | **new** — pure orb-state map (port of web `orbState.ts`) | low (pure) |
| `macos/Lopi/Store/Transcript.swift` | **new** — block model + builder over `LiveAgent.logTail` | low–med |
| `macos/Lopi/Views/Forge/TranscriptView.swift` | **new** — chat body (markdown, tool/thinking `DisclosureGroup`, chips, caret) | med |
| `macos/Lopi/Views/Forge/AgentPaneView.swift` | **rewritten** — full-pane chat + corner orb + `matchedGeometryEffect` | **high** |
| `macos/Lopi/Components/KonjoOrb.swift` | motion params (spinSpeed/pulseRate/glow/turbulence/special/glowColor) | med |
| `macos/Lopi/Components/ForgeOrb.metal` | 4 new uniforms (pulseRate, glow, turbulence, krypto) + jade halo | **high** (shader arg order must match the Swift call exactly) |
| `macos/Lopi/Components/MarkdownLogView.swift` | red/green diff gutter in `CodeBlockView` | low |
| `macos/Lopi/Theme/KonjoTheme.swift` | new orb-state hues (plasma/violet/violetBright/mint/roseMuted) | low |
| `macos/Lopi/Store/LiveState.swift` | `PhaseStyle.color` recolor (Testing→violet, etc.) | low |

## Known caveats I shipped deliberately (don't "fix" these blindly)

1. **No SPM MarkdownUI** — I used native `AttributedString(markdown:)` +
   `MarkdownLogView`. If you'd rather have MarkdownUI's richer rendering, add it
   to `project.yml` and re-run xcodegen — but it's optional, not a bug.
2. **Text-wrap is a reserved L-inset**, not a TextKit `exclusionPaths` circle
   (SwiftUI `Text` can't float-wrap). True circular wrap is a follow-up.
3. **Tool accordions show call + args, not the result body** — `tool_result`
   isn't on `LiveAgent.logTail`. To add results, thread them into the transcript
   (see "Optional deepening" below).
4. **Shift+Enter newline** uses a multi-line `TextField(axis:.vertical)`; exact
   "Enter sends / Shift+Enter newline" parity needs a custom `NSTextView`.

## Highest-likelihood compile snags (check these first)

- **Metal arg order**: `ForgeOrb.metal`'s `forgeOrb(...)` signature ends with
  `..., float pulseRate, float glow, float turbulence, float krypto`. The
  `ShaderLibrary.forgeOrb(...)` call in `KonjoOrb.swift` must pass those four
  `.float(...)` args **in that exact order, last**. If the orb renders blank or
  miscolored at runtime, this is the cause.
- **`matchedGeometryEffect`** across the two `if let agent` branches in
  `AgentPaneView.bodyArea` (same `id: "orb"`, same `@Namespace orbNS`). If the
  compiler complains about branch types, wrap both in the same container shape.
- **`TextField(text:axis:.vertical)`** needs the macOS-13 deployment target
  (precedent exists in `CronView.swift`/`TasksView.swift`).
- Removed from `AgentPaneView`: the old `logPanel`/`logStrip`/`WaitingDots`/
  `PaneLayout`-based `orbSize`. Confirm nothing else referenced them (they were
  `private`).

---

## PROMPT FOR CLAUDE CODE (paste this)

> You are validating the macOS half of PR #53 on this M3. Work the Konjo loop:
> kill-tests gate the build, then verify each gate, and STOP + report if a
> kill-test fails. Do not touch the web side or any Rust.
>
> **Setup**
> 1. `git fetch origin && git checkout claude/chat-render-living-orb-713jf5 && git pull`
> 2. `cd macos && xcodegen generate`
>
> **K1 — make it build (gate)**
> 3. `xcodebuild -project Lopi.xcodeproj -scheme Lopi -destination 'platform=macOS' build`
>    (project/target/scheme are all `Lopi`; entry point `Lopi/LopiApp.swift`).
>    Fix compile errors **minimally**, preferring
>    the smallest change that preserves the intent in `artifacts/RENDER_REPORT.md`.
>    Check the "Highest-likelihood compile snags" list in
>    `artifacts/MACOS_VALIDATION_HANDOFF.md` first. Commit each fix with
>    `fix(macos): …` and note it.
> 4. Run the test target (it exists — `macos/LopiTests/`):
>    `xcodebuild -project Lopi.xcodeproj -scheme Lopi -destination 'platform=macOS' test`.
>    `AgentEventGoldenTests` must stay green (the decoder was untouched).
>
> **Run it** — launch the app against a live backend so real events flow:
> 5. In one terminal: `cargo run -- sail` (starts the dashboard + WS on :3000).
> 6. Launch the built `Lopi.app`. Submit a real goal in a pane.
>    (If no Claude subscription is handy, the web demo path doesn't exist on
>    macOS — drive it with a real `lopi run`/`sail` session.)
>
> **Verify each gate (screenshot every one):**
> - **G2 render**: assistant markdown renders; fenced code shows a language label
>   + monospace box; a ```diff``` block shows green/red lines; tool calls are
>   collapsed `DisclosureGroup`s that expand to args; a caret trails the open
>   streaming text; thinking is collapsed by default.
> - **G3 layout**: the transcript fills the pane; the composer is pinned at the
>   bottom; the orb floats bottom-right above the composer, **≤ 300px** (measure
>   it); confirm the reserved L-inset keeps text off the orb (note honestly that
>   it does **not** wrap around the circle — that's expected).
> - **G4 orb state map**: drive a session through states and confirm colors +
>   motion match the table in `RENDER_REPORT.md` — Planning **ice**, Implementing
>   **plasma** + fast/turbulent, Testing **violet** (NOT yellow), Awaiting
>   **yellow/orange** + slow continual spin + attentionPulse, Completed **jade**
>   kryptonite halo slowing to a drift, Failed **rose** hardStop (no spin),
>   RollingBack **ember** reverseSpin, rate-limited **flame** stutter. Capture a
>   short screen recording.
> - **G5 absorption**: start from an idle/empty pane (large centered orb), mount a
>   session, and confirm the orb **travels + shrinks** into the corner in one
>   spring (`matchedGeometryEffect`), then keeps animating. Toggle
>   System Settings → Accessibility → Reduce Motion and confirm it cuts cleanly.
> - **G6 perf**: a long transcript scrolls smoothly; the continual orb animation
>   doesn't stutter. Note timing if measurable.
> - **G7 parity**: open the same session on web (`cargo run -- sail`, browser) and
>   macOS; confirm block order, collapse behavior, and orb state match.
>
> **Report**: append a `## macOS validation (M3)` section to
> `artifacts/RENDER_REPORT.md` with each gate PASS/FAIL + evidence (measured orb
> px, screenshots, the recording, any fixes made). Commit + push to the same
> branch. Then comment a one-line summary on PR #53. If you hit a wall, push what
> builds and report exactly where you're stuck — don't fake a green.

---

## Optional deepening (only if you have time after G1–G7)

- **Tool results in accordions**: add `transcript: [TranscriptBlock]` to
  `LiveAgent` and fold events in `AppModel+Live.ingest` (one call site), pairing
  `toolResult` back to its `toolCall` exactly like the web reducer
  (`web/src/lib/stores/transcript.ts`), instead of rebuilding from `logTail`.
- **True text-wrap**: an `NSViewRepresentable` over `NSTextView` with
  `NSTextContainer.exclusionPaths = [bottom-right circle]` (TextKit 2).
- **Shift+Enter**: a custom `NSTextView`-backed composer for exact send/newline
  semantics.

Spec of record: `artifacts/RENDER_REPORT.md` (kill-tests, gates, the shipped
status→color/motion map, lib choices, orb clamp). PR: konjoai/lopi#53.
