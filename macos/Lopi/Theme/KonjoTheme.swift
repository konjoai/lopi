import SwiftUI

/// The Konjo palette + typography, matching the web "Forge" dashboard exactly
/// (hex values from `crates/lopi-ui/src/placeholder.html`). The macOS app is
/// dark-first like the web UI.
enum Konjo {
    // Backgrounds
    static let bg = Color(hex: 0x06060F)
    static let bg1 = Color(hex: 0x0C0C1A)
    static let bg2 = Color(hex: 0x14142A)

    // Text
    static let fg = Color(hex: 0xE8E8F5)
    static let fgDim = Color(hex: 0xE8E8F5).opacity(0.62)
    static let fgMute = Color(hex: 0xE8E8F5).opacity(0.32)

    // Accents
    static let konjo = Color(hex: 0x7C3AED) // primary purple
    static let konjo2 = Color(hex: 0xA78BFA) // lighter purple
    static let ok = Color(hex: 0x5BE39B) // success green
    static let warn = Color(hex: 0xF59E0B) // amber
    static let err = Color(hex: 0xEF4444) // red

    // Hairlines
    static let line = Color.white.opacity(0.08)
    static let line2 = Color.white.opacity(0.14)

    // Typography
    static func mono(_ size: CGFloat, weight: Font.Weight = .regular) -> Font {
        .system(size: size, weight: weight, design: .monospaced)
    }

    static func sans(_ size: CGFloat, weight: Font.Weight = .regular) -> Font {
        .system(size: size, weight: weight, design: .default)
    }
}

extension Color {
    /// Build a `Color` from a 24-bit RGB hex literal, e.g. `0x7C3AED`.
    init(hex: UInt32) {
        let r = Double((hex >> 16) & 0xFF) / 255.0
        let g = Double((hex >> 8) & 0xFF) / 255.0
        let b = Double(hex & 0xFF) / 255.0
        self.init(.sRGB, red: r, green: g, blue: b, opacity: 1.0)
    }
}

/// A panel container matching the web UI: subtle fill, 1px hairline border,
/// 10pt corner radius, generous padding.
struct KonjoPanel<Content: View>: View {
    @ViewBuilder var content: Content

    var body: some View {
        content
            .padding(20)
            .background(Konjo.bg1)
            .overlay(
                RoundedRectangle(cornerRadius: 10)
                    .stroke(Konjo.line, lineWidth: 1)
            )
            .clipShape(RoundedRectangle(cornerRadius: 10))
    }
}
