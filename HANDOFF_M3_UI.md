# Handoff → Claude Code on M3: Forge UI continuation

**Paste everything below the line into Claude Code running locally on your M3 Mac.**
It is self-contained: it explains the current state, exactly how to build/run and
screenshot both UIs, what to verify first, and the bar to hold for what comes next.

---

You are continuing work on **lopi** — a Rust agent orchestrator with two front
ends: a **web UI** (`web/`, SvelteKit + Three.js) and a **native macOS app**
(`macos/`, SwiftUI). A large "Forge cockpit" sprint just landed on branch
`claude/konjo-lopi-aaju8e` (PR #34, all CI green). Your job is to **build and run
both UIs on this Mac, capture real screenshots, verify the macOS code actually
compiles, and then keep improving both** — fluid, clean, intuitive, animated,
*better than world-class*. Think Claude / Codex / Linear-tier polish, the Konjo
way (pure black, ember/ice/jade accents, mono for code, no chrome, every pixel
earns its place — see `LOPI_UI_VISION.md`).

## 0. Get the branch
```bash
git fetch origin claude/konjo-lopi-aaju8e
git checkout claude/konjo-lopi-aaju8e
git pull origin claude/konjo-lopi-aaju8e
```

## 1. What just shipped (read these first)
- `LOPI_UI_VISION.md` — the design north star (the Forge, palette, principles).
- `CHANGELOG.md` (top "Forge multi-agent cockpit" entry) — what changed and why.
- The **deletion-bug fix**: closing a pane now *parks* a session in the sidebar;
  deleting *tombstones* it so the WebSocket snapshot can't resurrect it.
  - Web: `web/src/lib/stores/layout.ts` + pure `layout-core.ts` (32 unit tests).
  - macOS: `macos/Lopi/Store/PaneLayout.swift` (same model, UserDefaults-backed).

New web components: `SessionSidebar.svelte`, `TileGrid.svelte` (auto-tiling
resizable grid), `LaunchControls.svelte`, `ui/Dropdown.svelte`.
New macOS files: `Components/KonjoOrb.swift`, `Store/PaneLayout.swift`,
`Store/LaunchControls.swift`, `Views/Forge/{ForgeView,PaneGridView,AgentPaneView,
SessionSidebarView,LaunchControlsView}.swift`, and a new **Forge** nav section.

## 2. ⚠️ Verify the macOS build FIRST — it was written blind
The previous session ran on Linux with **no Swift/Xcode toolchain**, so the
SwiftUI above is **compile-unverified**. Before any new work, make it build and
fix whatever the compiler flags. High-risk spots to scrutinize:
- `KonjoOrb.swift` — `GraphicsContext` usage: `drawLayer { layer in … }` (inout),
  `layer.clip(to:)`, `.conicGradient`, `.blendMode = .plusLighter`,
  `addFilter(.blur(radius:))`. Confirm every signature on macOS 14.
- `PaneGridView.swift` — `ForEach(0..<n, id: \.self)` with dynamic `n`, the
  `DragGesture` resize math, and `NSCursor` under `#if canImport(AppKit)`.
- Observation patterns: `@State private var layout = PaneLayout()` /
  `LaunchControls()` (both `@Observable @MainActor`), `@Bindable var controls`
  passed parent→child, and the two-parameter `.onChange(of:) { _, new in }`.
- `ForgeView` adds a `.toolbar` while `RootView` already sets one — confirm they
  compose, not collide.

```bash
cd macos
brew install xcodegen          # if missing
xcodegen generate              # regenerates Lopi.xcodeproj from project.yml
xcodebuild -project Lopi.xcodeproj -scheme Lopi -destination 'platform=macOS' build
# then open in Xcode for live iteration:
open Lopi.xcodeproj
```
Run the app (⌘R). It lands on the **Forge** tab. With no server running it shows
empty panes + the idle orb; that's enough to validate layout, the orb animation,
resizable gutters, selectors, and the sidebar.

## 3. Run the web UI
```bash
cd web
npm install
npm run dev          # vite dev server, usually http://localhost:5173
```
The Forge auto-falls back to **mock agents after ~1.5s** if no backend is up, so
you get live-looking orbs/panes for screenshots immediately. For real data,
run the server in another terminal: `cargo run -- sail` (serves on :3000, web
talks to its `/ws`). Useful checks:
```bash
npm test             # 134 pure-logic tests
npm run check        # svelte-check (expect 0 errors)
npm run build        # production build into web/dist (embedded by the Rust bin)
```

## 4. Capture real screenshots (the deliverable)
**macOS** (whole window or region):
```bash
screencapture -o -l $(osascript -e 'tell app "Lopi" to id of window 1') ~/Desktop/lopi-macos-forge.png
# or interactive region: screencapture -i ~/Desktop/lopi-macos.png
```
**Web** — drive a real browser with Playwright for crisp, repeatable shots:
```bash
cd web && npx playwright install chromium
npx playwright screenshot --viewport-size=1600,1000 http://localhost:5173 ~/Desktop/lopi-web-forge.png
```
Take **before/after pairs** for every change. Capture: the 4-pane default grid,
2-/3-pane tiling, a resized split, the sessions sidebar (with a parked session),
the selectors open, and the orb mid-animation. Attach them in chat and to the PR
so the human can see the improvement, not just read about it.

## 5. The bar — "better than world-class"
Hold this standard on both platforms, and prove each claim with a screenshot or
short screen recording:
- **Fluid motion**: 60fps. Springy, interruptible transitions (SwiftUI
  `.spring`, Svelte `crossfade`/tweened). The orb should feel *alive* — react to
  phase changes, token pressure, success/failure (jade bloom / rose flare).
- **Intuitive interactions**: drag-to-reorder and resize must feel native;
  hovers, focus rings, empty/loading/error states all designed, not default.
- **Clean interface**: ruthless hierarchy, generous negative space, one focal
  point (the orb), mono numerics, no visual noise. Match the palette exactly.
- **Parity + identity**: web and macOS should feel like the same product — same
  language, each idiomatic to its platform.

### Concrete next candidates (pick high-impact, ship small PRs)
1. **macOS build-verify + visual polish pass** (do this first).
2. Animate pane add/remove + tile re-flow (currently instant — make it spring).
3. Orb: add an eviction ripple + a phase-transition aurora sweep.
4. Selectors: richer popovers (model descriptions, recent repos, branch
   autocomplete from the git repo).
5. Sidebar: drag a session from the sidebar directly into a specific pane;
   search/filter; status grouping.
6. Keyboard-first: ⌘1–4 focus panes, ⌘W close pane, ⌘⌥W delete session.

## 6. Working rules (Konjo)
- Branch `claude/konjo-lopi-aaju8e`; conventional commits; commit+push per slice.
- `cargo build`/`cargo test` green before committing **any** Rust; for web,
  `npm run check` + `npm test` green. Keep files ≤ 500 lines, fns ≤ 50.
- Known follow-up debt: `web/src/lib/stores/agents.ts` is 587 lines — split it.
- Don't conflate close-pane with delete-session; preserve the tombstone fix.
- Update `CHANGELOG.md` and screenshot every UI change.

Make it Konjo: beautiful, lean, immersive, alive.
