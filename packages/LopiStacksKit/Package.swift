// swift-tools-version:5.9
import PackageDescription

// The shared domain layer extracted from `macos/Lopi/Stacks/` — pure
// Foundation(+Observation) stack-orchestration logic (config types, run
// sequencer, goal pursuit, templates, pane ops) with zero SwiftUI/AppKit
// imports, so it builds identically for macOS and (per iOS-Research-1) iOS.
// Two files stayed behind in the app target because they aren't part of the
// domain: `StackTheme.swift` (a SwiftUI `Color` extension used only by
// `Views/Forge/*`) and `CardOrbState.swift` (Foundation-only itself, but reads
// `LiveAgent`/`ForgeOrbState` from `Store/`, which import SwiftUI) — see
// `docs/ops/IOS_RESEARCH_1_SPIKE.md` for the boundary reasoning.
let package = Package(
    name: "LopiStacksKit",
    platforms: [
        .macOS(.v14),
        .iOS(.v17),
    ],
    products: [
        .library(name: "LopiStacksKit", targets: ["LopiStacksKit"]),
    ],
    targets: [
        .target(name: "LopiStacksKit"),
        .testTarget(name: "LopiStacksKitTests", dependencies: ["LopiStacksKit"]),
    ]
)
