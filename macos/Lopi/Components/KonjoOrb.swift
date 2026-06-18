import SwiftUI

/// The Forge orb — a perpetually-morphing sphere of fire and ice that breathes
/// and reacts to live agent cognition. Pure SwiftUI `Canvas` (no Metal), so it
/// builds on macOS 14 and scales to many panes.
///
/// Live inputs:
///   - `phase`    → accent color (planning purple, testing amber, success green…)
///   - `activity` → plasma rotation speed + glow intensity (generation pressure)
///   - `pressure` → orbital turbulence + warmth (context-window fill)
struct KonjoOrb: View {
    var phase: String
    var activity: Double
    var pressure: Double
    var size: CGFloat = 120

    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private var accent: Color { PhaseStyle.color(phase) }

    var body: some View {
        TimelineView(.animation(minimumInterval: 1.0 / 60.0, paused: reduceMotion)) { timeline in
            let t = reduceMotion ? 0 : timeline.date.timeIntervalSinceReferenceDate
            Canvas { ctx, sz in draw(&ctx, sz, t) }
        }
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    }

    private func draw(_ ctx: inout GraphicsContext, _ sz: CGSize, _ t: TimeInterval) {
        let c = CGPoint(x: sz.width / 2, y: sz.height / 2)
        let r = min(sz.width, sz.height) / 2
        let breathe = 1 + 0.04 * sin(t * 1.4)
        let coreR = r * 0.76 * breathe
        let core = circle(c, coreR)

        // Soft outer halo — bigger and brighter as the agent generates.
        ctx.drawLayer { layer in
            layer.addFilter(.blur(radius: r * 0.34))
            layer.opacity = 0.45 + 0.35 * activity
            layer.fill(circle(c, coreR * 1.05), with: .color(accent))
        }

        // Core sphere: a bright ice highlight falling off into the accent.
        ctx.fill(core, with: .radialGradient(
            Gradient(colors: [Konjo.fg.opacity(0.92), accent, accent.opacity(0.12)]),
            center: CGPoint(x: c.x - coreR * 0.28, y: c.y - coreR * 0.32),
            startRadius: 0, endRadius: coreR * 1.15))

        // Rotating plasma — a conic fire/ice band, clipped to the sphere.
        ctx.drawLayer { layer in
            layer.clip(to: core)
            layer.blendMode = .plusLighter
            layer.opacity = 0.32 + 0.4 * activity
            layer.fill(core, with: .conicGradient(
                Gradient(colors: [Konjo.konjo2, Konjo.warn, accent, Konjo.konjo2]),
                center: c, angle: .radians(t * (0.4 + activity))))
        }

        // Orbiting hot/cold blobs — turbulence climbs with context pressure.
        for i in 0..<4 {
            let off = Double(i) / 4 * 2 * .pi
            let speed = 0.55 + Double(i) * 0.17 + activity
            let orbit = coreR * (0.30 + 0.13 * Double(i % 2)) * (1 + 0.4 * pressure)
            let p = CGPoint(
                x: c.x + CGFloat(cos(t * speed + off)) * orbit,
                y: c.y + CGFloat(sin(t * speed * 1.1 + off)) * orbit)
            let br = coreR * (0.20 + 0.07 * pressure)
            let tint = i % 2 == 0 ? Konjo.warn : Konjo.konjo2
            ctx.drawLayer { layer in
                layer.clip(to: core)
                layer.addFilter(.blur(radius: br * 0.65))
                layer.blendMode = .plusLighter
                layer.opacity = 0.5
                layer.fill(circle(p, br), with: .radialGradient(
                    Gradient(colors: [tint, tint.opacity(0)]),
                    center: p, startRadius: 0, endRadius: br))
            }
        }

        // Fresnel rim.
        ctx.stroke(core, with: .color(accent.opacity(0.7)), lineWidth: max(1, r * 0.02))
    }

    private func circle(_ center: CGPoint, _ radius: CGFloat) -> Path {
        Path(ellipseIn: CGRect(x: center.x - radius, y: center.y - radius,
                               width: radius * 2, height: radius * 2))
    }
}
