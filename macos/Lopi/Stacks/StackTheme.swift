import SwiftUI

// Stack-specific palette additions — the few tokens the web stack components use
// that the base `Konjo` palette (Forge's ice identity) doesn't carry. Kept as a
// small extension so the rest of the theme stays the single source of truth.
extension Konjo {
    /// The dock / config accent (`--stack-violet`, #B79BFF).
    static let stackViolet = Color(hex: 0xB79BFF)
    /// The alias-chip teal (`--stack-teal`, #00FFD4).
    static let stackTeal = Color(hex: 0x00FFD4)
    /// The repo-chip sky blue (`--stack-sky`, #66B3FF) — mirrors the web
    /// `ProvenanceChips.svelte` repo chip color.
    static let stackSky = Color(hex: 0x66B3FF)
    /// The budget-badge violet (`--konjo-violet` as used by the stack components).
    static let budgetViolet = Color(hex: 0xB388FF)
    /// The output well background (`--stack-outbg`).
    static let outBg = Color(hex: 0x0C1417)
    /// The pane panel fill (`--konjo-panel`).
    static let panel = Color(hex: 0x0A0D0F)
}

/// The per-facet accent a cardbar button / summary row uses, matching the web
/// component's per-icon color. Centralized so card + dock read identically.
enum FacetAccent {
    static let schedule = Konjo.ice
    static let guards = Konjo.sun
    static let evals = Konjo.jade
    static let config = Konjo.stackViolet
    static let goal = Konjo.flame
    static let iteration = Konjo.flame
}
