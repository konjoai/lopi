import SwiftUI

/// The signature Konjo backdrop: slow-drifting aurora blobs over the near-black
/// base. `intensity` (0...1, typically the fleet's aggregate activity) brightens
/// and speeds the field so the whole app visibly "breathes" when agents think.
struct AuroraBackground: View {
    var intensity: Double = 0.25
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private let blobs: [Blob] = [
        Blob(color: Konjo.ice, phase: 0.0, scale: 0.95, speed: 0.06),
        Blob(color: Konjo.konjo2, phase: 1.7, scale: 0.75, speed: 0.09),
        Blob(color: Konjo.jade, phase: 3.3, scale: 0.55, speed: 0.05),
        // Warm counter-tone keeps the cyan void from reading cold/flat —
        // the same ember pool the web ambient background uses.
        Blob(color: Color(hex: 0x3B1605), phase: 5.0, scale: 1.1, speed: 0.04),
    ]

    var body: some View {
        ZStack {
            Konjo.bg
            if reduceMotion {
                Canvas { ctx, size in render(ctx, size, t: 0) }
                    .opacity(0.6)
            } else {
                TimelineView(.animation) { timeline in
                    let t = timeline.date.timeIntervalSinceReferenceDate
                    Canvas { ctx, size in render(ctx, size, t: t) }
                }
            }
            // Subtle grain/vignette to seat the blobs into the dark base.
            RadialGradient(
                colors: [.clear, Konjo.bg.opacity(0.85)],
                center: .center, startRadius: 200, endRadius: 900
            )
        }
        .ignoresSafeArea()
    }

    private func render(_ ctx: GraphicsContext, _ size: CGSize, t: TimeInterval) {
        let glow = 0.10 + intensity * 0.35
        for blob in blobs {
            let drift = t * blob.speed * (0.5 + intensity)
            let x = (0.5 + 0.32 * cos(drift + blob.phase)) * size.width
            let y = (0.5 + 0.30 * sin(drift * 1.3 + blob.phase)) * size.height
            let r = blob.scale * min(size.width, size.height) * 0.55
            let rect = CGRect(x: x - r, y: y - r, width: r * 2, height: r * 2)
            let gradient = Gradient(colors: [
                blob.color.opacity(glow),
                blob.color.opacity(0),
            ])
            ctx.fill(
                Path(ellipseIn: rect),
                with: .radialGradient(gradient, center: CGPoint(x: x, y: y),
                                      startRadius: 0, endRadius: r)
            )
        }
    }

    private struct Blob {
        let color: Color
        let phase: Double
        let scale: Double
        let speed: Double
    }
}

extension View {
    /// A soft colored glow — the Konjo accent halo used on active elements.
    func konjoGlow(_ color: Color, radius: CGFloat = 12, active: Bool = true) -> some View {
        shadow(color: active ? color.opacity(0.55) : .clear, radius: radius)
            .shadow(color: active ? color.opacity(0.25) : .clear, radius: radius * 2)
    }

    /// Hairline-bordered translucent surface used by panels and pills.
    func konjoSurface(_ corner: CGFloat = 12, fill: Color = Konjo.bg1.opacity(0.7)) -> some View {
        background(fill)
            .overlay(RoundedRectangle(cornerRadius: corner).stroke(Konjo.line, lineWidth: 1))
            .clipShape(RoundedRectangle(cornerRadius: corner))
    }
}

/// A large metric that animates between values (rolling-counter feel).
struct RollingNumber: View {
    let value: Double
    var format: String = "%.0f"
    var font: Font = Konjo.sans(28, weight: .semibold)
    var color: Color = Konjo.fg

    var body: some View {
        Text(String(format: format, value))
            .font(font)
            .foregroundStyle(color)
            .contentTransition(.numericText(value: value))
            .animation(.snappy(duration: 0.5), value: value)
            .monospacedDigit()
    }
}

/// A small rounded tag.
struct Pill: View {
    let text: String
    var color: Color = Konjo.konjo2

    var body: some View {
        Text(text)
            .font(Konjo.mono(10, weight: .medium))
            .foregroundStyle(color)
            .padding(.horizontal, 7)
            .padding(.vertical, 2)
            .background(color.opacity(0.16))
            .clipShape(Capsule())
    }
}

/// Section heading in the cockpit — uppercase mono label with a trailing rule.
struct SectionLabel: View {
    let text: String
    var trailing: String?

    var body: some View {
        HStack(spacing: 10) {
            Text(text.uppercased())
                .font(Konjo.mono(10, weight: .semibold))
                .foregroundStyle(Konjo.fgMute)
                .tracking(1.5)
            Rectangle().fill(Konjo.line).frame(height: 1)
            if let trailing {
                Text(trailing)
                    .font(Konjo.mono(10))
                    .foregroundStyle(Konjo.fgMute)
            }
        }
    }
}
