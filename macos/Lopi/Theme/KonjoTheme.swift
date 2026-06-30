import SwiftUI

/// The Konjo palette + typography, matching the web "Forge" dashboard's
/// ice-cyan identity (hex values from `web/src/app.css`). Dark-first, near-black
/// void with an electric-cyan accent and a warm phase spectrum.
enum Konjo {
    // Backgrounds — neutral near-black void, matching the web's `--konjo-black`.
    static let bg = Color(hex: 0x0A0A0A) // --konjo-black
    static let bg1 = Color(hex: 0x101013) // panel fill
    static let bg2 = Color(hex: 0x16161B) // raised surface
    static let deep = Color(hex: 0x050505) // --konjo-deep (modals/wells)

    // Text — bright paper on black.
    static let fg = Color(hex: 0xF5F5F5) // --konjo-paper
    static let fgDim = Color(hex: 0xF5F5F5).opacity(0.62)
    static let fgMute = Color(hex: 0xF5F5F5).opacity(0.32)

    // ── Konjo spectrum (1:1 with the web palette) ───────────────────────────
    static let ice = Color(hex: 0x00D4FF) // --konjo-ice (primary accent)
    static let iceDeep = Color(hex: 0x0088AA)
    static let ember = Color(hex: 0xFF4500) // --konjo-ember
    static let flame = Color(hex: 0xFF9500) // --konjo-flame
    static let jade = Color(hex: 0x00FF9D) // --konjo-jade
    static let sun = Color(hex: 0xFFCC00) // --konjo-sun
    static let rose = Color(hex: 0xFF0066) // --konjo-rose

    // Orb-state palette (living-orb status map). Mirrors web app.css; yellow/
    // orange is reserved for awaiting and green for success, so Testing is
    // violet and Scoring bright-violet (K-collision).
    static let plasma = Color(hex: 0x5EE6FF) // Implementing — plasma cyan
    static let violet = Color(hex: 0x7C3AED) // Testing — violet
    static let violetBright = Color(hex: 0x9D5CFF) // Scoring / verifying
    static let mint = Color(hex: 0x3BE6C8) // Opening PR — pre-success mint
    static let roseMuted = Color(hex: 0xB04A6A) // Cancelled — muted rose

    // Semantic accents — alias onto the spectrum so existing call sites keep
    // working while reading as one cohesive Konjo identity.
    static let konjo = ice // primary accent (electric cyan)
    static let konjo2 = Color(hex: 0x5EE6FF) // lighter cyan (highlights/plasma)
    static let ok = jade // success
    static let warn = flame // warning / heat
    static let err = rose // error

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
