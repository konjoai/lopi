# IOS_RESEARCH_1_SPIKE — Swift package extraction, written not built

**Baseline:** `origin/main` @ `43f7cd5` (Loop Stack connect & test, v0.11.0) · **Date:** 2026-07-15
**Discipline, stated up front:** Swift does not compile on this host (Linux, no Xcode). Everything below is grep-verified against the source and reasoned from Swift's documented access-control rules — it is **not** a compiled result. Nothing here should be read as "this builds." The M3 pass (`xcodegen generate && xcodebuild -scheme Lopi test`, plus `cd packages/LopiStacksKit && swift test`) is the actual acceptance bar, same discipline as every macOS round since Ops-2.

## What this closes

Verify-4's R-1 package question: can `macos/Lopi/Stacks/` extract into a shared Swift package (for iOS reuse) as "a move, not a rewrite"? Verify-4 established the *test* layer was framework-free (60/60 ported, zero SwiftUI/AppKit). This spike re-verified that claim at the **source** layer, file by file, and found it's true for 15 of 17 files — not the whole directory as stated. The two exceptions are real, and are exactly the kind of thing a grep-based claim misses if nobody re-checks it.

## The package

`packages/LopiStacksKit/` — a standalone Swift Package Manager package (`Package.swift`, `swift-tools-version:5.9`, targets `.macOS(.v14)` + `.iOS(.v17)`), sibling to `macos/`, not nested inside it — a shared package belongs at repo scope, not inside the one platform target that predates it. `macos/project.yml` gained a `packages:` block (local path dependency) and the `Lopi` target now depends on it; xcodegen has not been re-run (that's an M3 step, `xcodegen generate` regenerates `Lopi.xcodeproj` from `project.yml`).

**Moved via `git mv`** (history-preserving) — 15 files, 2448 lines, into `packages/LopiStacksKit/Sources/LopiStacksKit/`:
`OptionMenu`, `RepoMenu`, `StackConfigTypes`, `StackCron`, `StackGoal`, `StackOps`, `StackPaneOps`, `StackPayload`, `StackRun`, `StackRunControls`, `StackStore`, `StackSummaries`, `StackTemplateStore`, `StackTemplates`, `StackTypes`.

**Ported, not rewritten** — the three test files Verify-4 counted (`StackStoreTests`/`StackGoalTests`/`StackRunTests`, 1,080 lines, the 60 assertions) moved into `packages/LopiStacksKit/Tests/LopiStacksKitTests/` with a one-line import change (`@testable import Lopi` → `@testable import LopiStacksKit`) and are otherwise byte-identical. Confirmed before moving them: none of the three reference `LiveAgent`/`ForgeOrbState`/`AppModel`/SwiftUI/AppKit — the "in-memory mock" `StackRunTests` uses is self-contained against `StackRunSeams`, which moved with it.

## The two files that did NOT move — the real finding

Re-verifying the directory file-by-file (not trusting "the whole layer is framework-free" as stated) surfaced two exceptions:

- **`StackTheme.swift`** stays in `macos/Lopi/Stacks/`. It `import SwiftUI` directly (a `Color`/`Konjo` palette extension: `stackViolet`, `stackTeal`, `budgetViolet`, `outBg`, `panel`, `FacetAccent`) and is consumed exclusively by `Views/Forge/*.swift` (12 files, confirmed by grep — nothing outside `Views/Forge` references it). It was never domain logic; it's UI theming that happened to live in the same directory.
- **`CardOrbState.swift`** stays too, and this one is the actual compile-risk flag for M3. It `import Foundation` only — no SwiftUI — so a directory-level import scan would have called it clean. But its `CardOrb.state(for:in:)` reads `LiveAgent` (`Store/LiveState.swift`) and `ForgeOrbState`/`OrbStateMap` (`Store/ForgeOrbState.swift`), and **both of those files `import SwiftUI`**. Moving `CardOrbState.swift` into `LopiStacksKit` as-is would pull a transitive SwiftUI dependency into the package, defeating the entire point of the extraction. Left in the app target, unchanged. **If iOS-Research-1 needs orb-state resolution in the shared package later, the real fix is a protocol in `LopiStacksKit` (e.g. `LiveAgentStatusProviding`) that `LiveAgent`/`OrbStateMap` conform to from the app side — not moving this file as-is.** Not built here: that's a real design decision for a future sprint, not a mechanical port.

Net: the package is 15 files / 2448 lines, not 17 / ~2500. "Framework-free" was true for the test layer (Verify-4) and is true for 15 of 17 source files — worth stating precisely rather than repeating the rounder claim.

## The access-control work (the part a "move" quietly isn't)

Every symbol in the moved files defaulted to Swift's `internal` access, which was invisible outside the file only because Views/Store lived in the *same module*. Splitting into a separate package makes that boundary real: anything the app still touches needs `public`, and — the sharp edge — **Swift never synthesizes a `public` memberwise initializer, even for a fully-`public` struct with fully-`public` properties.** Every struct without a hand-written `init` needed one added by hand to stay constructible from the app target.

Mechanical rule applied, uniformly, across all 15 files: every top-level type/func/constant → `public`; every member not already explicitly `private`/`fileprivate` → `public`; every struct without an explicit `init` got one, mirroring the implicit memberwise initializer's parameter names/order/defaults exactly; every class's `init` → `public init`; two `extension` blocks (`PaneDefaults`, `StackRunEngine`) became `public extension` rather than member-by-member. Where genuinely unsure whether something needed exposing, the rule defaulted to `public` — over-exposing is a harmless, tightenable-later choice with no compiler here to catch an under-exposed miss; under-exposing is a guaranteed compile error at the one point this can actually be checked.

Verified after the fact (mechanically, via grep — not compiled):
- Every file: brace-balanced, `import Foundation`/`import Observation` only, zero non-public top-level declarations remaining.
- Spot-checked the two structs most likely to drift — `StackRunSeams` (7 closure-typed properties: `panes`/`updateCard`/`createTask`/`waitForTerminal`/`score`/`createSchedule`/`reorderPaneCards`) and `StackConfig` — against their real call sites (`Store/AppModel+Stacks.swift`'s `makeStackSeams()`, `StackConfigTypes.swift`'s `defaultStackConfig()`). Both match the new `public init` signatures label-for-label, argument-for-argument.
- `import LopiStacksKit` added to all 24 app-target files a symbol-usage sweep found referencing package types (`Views/Forge/*` 12 files, `Store/*` 4 files, `Networking/LopiClient.swift`, `Stacks/CardOrbState.swift`, plus `Views/Admin/BudgetView.swift`, `Views/Dashboard/DashboardView.swift`, `Views/Loop/LoopView.swift`, `Views/RootView.swift` — a wider set than "Views/Forge + Store," found only by actually grepping the whole app tree against the package's public symbol list rather than assuming the two obvious directories were the whole surface).

**What "written not built" cannot rule out**, named rather than guessed past:
- A closure-type mismatch in a hand-written `public init` that only a real type-checker would catch (the `StackRunSeams`/`StackConfig` spot-checks passed; the other ~25 structs were not individually cross-checked against every call site).
- Any place a property is used as an inferred generic constraint or protocol-witness in a way access-control grep can't see.
- Whether `xcodegen generate` cleanly regenerates `Lopi.xcodeproj` from the new `packages:` block in `project.yml` — untested, since xcodegen doesn't run on this host either.

## For the M3 pass

1. `xcodegen generate` (regenerate `Lopi.xcodeproj` with the new package dependency), then `xcodebuild -scheme Lopi build`. Expect it to fail readably at the first real gap (matching the discipline of every prior round — one root cause, not a pile of typos, has been the pattern so far).
2. `cd packages/LopiStacksKit && swift test` — the actual bar, mirroring the ported `StackStoreTests`/`StackGoalTests`/`StackRunTests` 60/60 acceptance from Verify-4.
3. If `CardOrbState.swift`'s design (protocol-based orb-state seam) becomes real work, scope it as its own decision — not a default while fixing an unrelated compile error.
