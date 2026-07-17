import SwiftUI

/// The browser-local accent-theme picker's three swatches — mirrors web's
/// `stores/theme.ts` `Theme`/`THEMES` exactly (same ids, same swatch hex
/// values). Web overrides only `--konjo-accent`/`--konjo-accent-rgb` (a
/// handful of chrome-level touch points: dropdowns, sparklines, stat cards,
/// the layout shell) rather than every hardcoded palette color — the macOS
/// port is scoped the same way, via `AppModel.accentColor`, not a full
/// app reskin. `.ember` here is a distinct color from the existing
/// `Konjo.ember` constant (`0xFF4500`, used for phase/retry styling) — do
/// not conflate the two.
enum AccentTheme: String, CaseIterable, Identifiable {
    case ice, ember, jade

    var id: String { rawValue }

    var label: String {
        switch self {
        case .ice: return "Ice"
        case .ember: return "Ember"
        case .jade: return "Jade"
        }
    }

    var swatch: Color {
        switch self {
        case .ice: return Color(hex: 0x00D4FF)
        case .ember: return Color(hex: 0xFF9500)
        case .jade: return Color(hex: 0x00FF9D)
        }
    }

    private static let storageKey = "lopi-theme"

    /// The persisted theme, or `.ice` (the implicit default, matching web's
    /// "no `data-theme` attribute" default) if nothing's been saved yet.
    static func load() -> AccentTheme {
        UserDefaults.standard.string(forKey: storageKey).flatMap(AccentTheme.init(rawValue:)) ?? .ice
    }

    /// Persist this choice — same storage key web uses (`localStorage['lopi-theme']`),
    /// just backed by `UserDefaults` instead of the browser.
    func persist() {
        UserDefaults.standard.set(rawValue, forKey: Self.storageKey)
    }
}
