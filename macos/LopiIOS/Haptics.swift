import UIKit

/// Centralized haptic feedback — every tappable control in `LopiIOS/Views`
/// routes through one of these so the feel is consistent app-wide, rather
/// than each call site instantiating (and re-`prepare()`-ing) its own
/// generator.
enum Haptics {
    private static let light = UIImpactFeedbackGenerator(style: .light)
    private static let medium = UIImpactFeedbackGenerator(style: .medium)
    private static let selectionGen = UISelectionFeedbackGenerator()
    private static let notificationGen = UINotificationFeedbackGenerator()

    /// The default for nearly every button — toggles, navigation, menu items.
    static func tap() {
        light.prepare()
        light.impactOccurred()
    }

    /// A heavier tap for primary/destructive actions — run/stop, delete, drain.
    static func impact() {
        medium.prepare()
        medium.impactOccurred()
    }

    /// A discrete value change — steppers, autocomplete picks, tab switches.
    static func selection() {
        selectionGen.prepare()
        selectionGen.selectionChanged()
    }

    static func warning() {
        notificationGen.notificationOccurred(.warning)
    }
}
