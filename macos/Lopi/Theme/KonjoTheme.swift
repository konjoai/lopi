import SwiftUI

/// The Konjo palette + typography, matching the web "Forge" dashboard exactly.
/// Hex values mirror the `:root` custom properties in `web/src/app.css` — a
/// near-black canvas with an ice/cyan primary accent (NOT purple). The macOS
/// app is dark-first like the web UI.
enum Konjo {
    // ── Core palette (web app.css :root) ────────────────────────────────────
    static let black = Color(hex: 0x0A0A0A) // --konjo-black (page background)
    static let deep = Color(hex: 0x050505) // --konjo-deep (panels / top bar)
    static let paper = Color(hex: 0xF5F5F5) // --konjo-paper (primary text)
    static let ice = Color(hex: 0x00D4FF) // --konjo-ice (primary accent)
    static let iceDeep = Color(hex: 0x0088AA) // --konjo-ice-deep
    static let ember = Color(hex: 0xFF4500) // --konjo-ember (working / hot)
    static let flame = Color(hex: 0xFF9500) // --konjo-flame
    static let jade = Color(hex: 0x00FF9D) // --konjo-jade (success / live)
    static let sun = Color(hex: 0xFFCC00) // --konjo-sun (queued / testing)
    static let rose = Color(hex: 0xFF0066) // --konjo-rose (failed / offline)

    // ── Semantic aliases (keep existing call sites stable) ──────────────────
    static let bg = black
    static let bg1 = deep
    static let bg2 = Color(hex: 0x0F0F12)
    static let fg = paper
    static let fgDim = paper.opacity(0.62)
    static let fgMute = paper.opacity(0.40)
    static let konjo = ice // primary accent
    static let konjo2 = Color(hex: 0x00FFD4) // planning teal
    static let ok = jade
    static let warn = sun
    static let err = rose

    // ── Hairlines (web uses white/5–10%) ────────────────────────────────────
    static let line = Color.white.opacity(0.06)
    static let line2 = Color.white.opacity(0.10)

    // ── Typography ──────────────────────────────────────────────────────────
    static func mono(_ size: CGFloat, weight: Font.Weight = .regular) -> Font {
        .system(size: size, weight: weight, design: .monospaced)
    }

    static func sans(_ size: CGFloat, weight: Font.Weight = .regular) -> Font {
        .system(size: size, weight: weight, design: .default)
    }

    // ── Phase + status colors (mirror PHASE_COLORS in stores/agents.ts) ──────

    /// Color for a cognition phase name (Boot / Discovery / … / Conclusion).
    static func phaseColor(_ phase: String) -> Color {
        switch phase.lowercased() {
        case "boot": return paper
        case "discovery": return ice
        case "planning": return konjo2
        case "implementation", "implementing": return ember
        case "testing", "scoring": return sun
        case "conclusion", "done": return jade
        default: return ice
        }
    }

    /// LED/dot color for a task status, matching AgentCard's `statusDot`.
    static func statusColor(_ status: String) -> Color {
        switch status.lowercased() {
        case "running", "implementing", "planning", "discovery": return jade
        case "queued": return sun
        case "success", "completed", "done": return jade.opacity(0.5)
        case "failed", "rolledback", "rolled_back": return rose
        case "cancelled": return ice.opacity(0.3)
        default: return paper.opacity(0.3)
        }
    }

    /// A representative phase name for a coarse task status, so the orb can be
    /// tinted from REST data that only carries a status string.
    static func statusPhase(_ status: String) -> String {
        switch status.lowercased() {
        case "queued": return "Boot"
        case "running", "implementing": return "Implementation"
        case "planning": return "Planning"
        case "discovery": return "Discovery"
        case "testing", "scoring": return "Testing"
        case "success", "completed", "done": return "Conclusion"
        case "failed", "rolledback", "rolled_back": return "Implementation"
        default: return "Discovery"
        }
    }
}

extension Color {
    /// Build a `Color` from a 24-bit RGB hex literal, e.g. `0x00D4FF`.
    init(hex: UInt32) {
        let r = Double((hex >> 16) & 0xFF) / 255.0
        let g = Double((hex >> 8) & 0xFF) / 255.0
        let b = Double(hex & 0xFF) / 255.0
        self.init(.sRGB, red: r, green: g, blue: b, opacity: 1.0)
    }
}

/// The ambient starfield backdrop from `app.css` `body::before` — a black
/// canvas washed with two faint radial gradients (ice top-left, ember
/// bottom-right). Place behind every screen for the web's sense of depth.
struct KonjoBackground: View {
    var body: some View {
        ZStack {
            Konjo.black
            RadialGradient(
                colors: [Konjo.ice.opacity(0.05), .clear],
                center: UnitPoint(x: 0.2, y: 0.3), startRadius: 0, endRadius: 520
            )
            RadialGradient(
                colors: [Konjo.ember.opacity(0.04), .clear],
                center: UnitPoint(x: 0.8, y: 0.75), startRadius: 0, endRadius: 520
            )
        }
        .ignoresSafeArea()
    }
}

/// A panel container matching the web UI: translucent `konjo-deep` fill, a 1px
/// white hairline border, and a 10pt corner radius (`rounded-lg`).
struct KonjoPanel<Content: View>: View {
    var padding: CGFloat = 16
    @ViewBuilder var content: Content

    var body: some View {
        content
            .padding(padding)
            .background(Konjo.deep.opacity(0.6))
            .overlay(
                RoundedRectangle(cornerRadius: 10)
                    .stroke(Konjo.line2, lineWidth: 1)
            )
            .clipShape(RoundedRectangle(cornerRadius: 10))
    }
}
