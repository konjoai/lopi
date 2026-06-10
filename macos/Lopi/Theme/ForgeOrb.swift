import SwiftUI

/// "The Forge" — lopi's signature visualization, ported from the WebGL sphere
/// in `web/src/lib/forge/Forge.svelte`. A real-time morphing orb of fire and
/// ice whose aura takes the current phase color and whose pulse rate tracks
/// agent activity. This is a layered-gradient approximation of the GLSL shader:
/// an ice/phase core, a rotating fire-ice swirl, a specular highlight, and a
/// Fresnel rim — all breathing on a `TimelineView` clock.
struct ForgeOrb: View {
    /// Aura tint — the agent's current phase color.
    var phaseColor: Color
    /// Generation intensity 0…1; drives pulse rate and ember brightness.
    var activity: Double = 0.5
    /// Context-window pressure 0…1; drives swirl turbulence.
    var pressure: Double = 0.4
    /// Diameter in points.
    var size: CGFloat = 140
    /// Whether the agent is actively working (brighter aura).
    var running: Bool = false

    var body: some View {
        TimelineView(.animation) { timeline in
            let t = timeline.date.timeIntervalSinceReferenceDate
            let pulse = 1 + 0.035 * sin(t * (1.4 + activity * 2.4))
            ZStack {
                core
                swirl(rotation: .degrees(t.truncatingRemainder(dividingBy: 360) * (12 + pressure * 26)))
                highlight
                rim(rotation: .degrees(-t.truncatingRemainder(dividingBy: 360) * 8))
            }
            .frame(width: size, height: size)
            .scaleEffect(pulse)
            .shadow(color: phaseColor.opacity(running ? 0.55 : 0.32),
                    radius: running ? 26 : 16)
        }
        .frame(width: size, height: size)
    }

    /// Fire/ice core: a bright phase-tinted center falling off to ice-deep then
    /// near-black at the limb.
    private var core: some View {
        Circle().fill(
            RadialGradient(
                colors: [phaseColor.opacity(0.95), Konjo.iceDeep.opacity(0.9), Konjo.black],
                center: .center, startRadius: 1, endRadius: size * 0.56
            )
        )
    }

    /// Rotating angular swirl evoking the fire/ice domain boundary.
    private func swirl(rotation: Angle) -> some View {
        Circle().fill(
            AngularGradient(
                colors: [Konjo.ice, phaseColor, Konjo.ember, Konjo.ice],
                center: .center
            )
        )
        .blendMode(.screen)
        .opacity(0.30 + activity * 0.20)
        .rotationEffect(rotation)
        .blur(radius: size * 0.06)
        .clipShape(Circle())
    }

    /// Off-axis specular highlight — the "wet glass" sheen.
    private var highlight: some View {
        Circle().fill(
            RadialGradient(
                colors: [Color.white.opacity(0.5), .clear],
                center: UnitPoint(x: 0.36, y: 0.30), startRadius: 1, endRadius: size * 0.34
            )
        )
        .blendMode(.screen)
        .clipShape(Circle())
    }

    /// Fresnel rim — only the grazing edge glows, in phase + ice.
    private func rim(rotation: Angle) -> some View {
        Circle().strokeBorder(
            AngularGradient(
                colors: [phaseColor.opacity(0), phaseColor, Konjo.ice, phaseColor.opacity(0)],
                center: .center
            ),
            lineWidth: 2
        )
        .rotationEffect(rotation)
        .blur(radius: 1)
    }
}
