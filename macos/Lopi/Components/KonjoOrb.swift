import SwiftUI

/// The Forge orb — an ever-rotating, noise-morphing sphere of fire and ice,
/// driven by live agent cognition. This is a 1:1 port of the web UI's WebGL
/// orb: the real GLSL shader (`web/src/lib/forge/Forge.svelte`) recompiled as
/// a Metal `colorEffect` (`ForgeOrb.metal`), so the silhouette genuinely
/// deforms and the surface spins — not a flat 2D approximation.
///
/// Live inputs:
///   - `phase`    → accent color driving the whole palette (cyan planning,
///                  ember implementing, gold testing, jade conclusion…)
///   - `activity` → pulse rate + ember brightness (generation intensity)
///   - `pressure` → surface turbulence + silhouette displacement (context fill)
///   - `health`   → overall warmth (recent success rate)
struct KonjoOrb: View {
    var phase: String
    var activity: Double
    var pressure: Double
    var health: Double = 0.85
    var size: CGFloat = 120

    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private var accent: Color { PhaseStyle.color(phase) }

    var body: some View {
        TimelineView(.animation(minimumInterval: 1.0 / 60.0, paused: reduceMotion)) { timeline in
            let t = reduceMotion ? 0 : timeline.date.timeIntervalSinceReferenceDate
            Rectangle()
                .fill(.black)
                .colorEffect(
                    ShaderLibrary.forgeOrb(
                        .boundingRect,
                        .float(Float(t.truncatingRemainder(dividingBy: 3600))),
                        .float(Float(min(max(pressure, 0), 1))),
                        .float(Float(min(max(activity, 0), 1))),
                        .float(Float(min(max(health, 0), 1))),
                        .color(accent)
                    )
                )
        }
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    }
}
