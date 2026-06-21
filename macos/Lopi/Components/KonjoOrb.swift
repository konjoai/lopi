import SwiftUI

/// The Forge orb — an ever-rotating, noise-morphing sphere of fire and ice that
/// also *reacts* to live stimuli, a 1:1 port of the web UI's WebGL orb. The
/// surface shader lives in `ForgeOrb.metal`; this view drives its uniforms and
/// the excitement envelope (shake → spin-up → colored flare → settle).
///
/// Live inputs:
///   - `phase`    → accent color driving the whole palette
///   - `activity` → pulse rate + ember brightness (generation intensity)
///   - `pressure` → surface turbulence + silhouette displacement (context fill)
///   - `health`   → overall warmth (recent success rate)
///   - `stimulus` / `stimulusKind` → bump the timestamp to make the orb react:
///       request → ember + shake, success → jade bloom, failure → rose + shake.
struct KonjoOrb: View {
    var phase: String
    var activity: Double
    var pressure: Double
    var health: Double = 0.85
    var stimulus: Date = .distantPast
    var stimulusKind: String = "request"
    var size: CGFloat = 120

    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    /// How long a single stimulus burns (matches the web's EXCITE_DURATION_MS).
    private let exciteDuration: Double = 2.5
    private var accent: Color { PhaseStyle.color(phase) }

    var body: some View {
        TimelineView(.animation(minimumInterval: 1.0 / 60.0, paused: reduceMotion)) { timeline in
            let f = frame(at: timeline.date)
            Rectangle()
                .fill(.black)
                .colorEffect(
                    ShaderLibrary.forgeOrb(
                        .boundingRect,
                        .float(f.time),
                        .float(Float(min(max(pressure, 0), 1))),
                        .float(Float(min(max(activity, 0), 1))),
                        .float(Float(min(max(health, 0), 1))),
                        .color(accent),
                        .float(f.spin),
                        .float(f.excite),
                        .color(exciteColor)
                    )
                )
                .offset(x: f.ox, y: f.oy)
        }
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    }

    /// Per-frame uniforms, computed statelessly from the clock + stimulus.
    private struct Frame {
        var time: Float
        var spin: Float
        var excite: Float
        var ox: CGFloat
        var oy: CGFloat
    }

    private func frame(at date: Date) -> Frame {
        let nowRef = date.timeIntervalSinceReferenceDate
        let t = reduceMotion ? 0 : nowRef

        // Excitement envelope — linear decay from the stimulus timestamp.
        let since = date.timeIntervalSince(stimulus)
        let excite = (!reduceMotion && since >= 0) ? max(0, 1 - since / exciteDuration) : 0

        // Rotation: gentle base drift plus a small analytic spin-up that
        // accrues while a stimulus burns (integral of the smoothstepped boost),
        // then holds. Kept subtle so frequent stimuli don't whip the orb.
        let baseRate = 0.22
        var spin = t * baseRate
        if !reduceMotion && since >= 0 {
            let a = min(max(since / exciteDuration, 0), 1)
            let inv = 1 - a
            let area = 0.5 - (inv * inv * inv - 0.5 * inv * inv * inv * inv)
            spin += baseRate * 1.8 * exciteDuration * area
        }

        // Shake: a faint front-loaded nudge (excite³) for requests/failures —
        // a reaction, not a rattle.
        let shakeAmp = shakes ? excite * excite * excite * Double(size) * 0.008 : 0
        return Frame(
            time: Float(t.truncatingRemainder(dividingBy: 3600)),
            spin: Float(spin.truncatingRemainder(dividingBy: 2 * .pi)),
            excite: Float(smoothstep01(excite)),
            ox: CGFloat(sin(nowRef * 53.0) * shakeAmp),
            oy: CGFloat(cos(nowRef * 61.0) * shakeAmp)
        )
    }

    /// Whether the current stimulus kind rattles the orb (success does not).
    private var shakes: Bool { stimulusKind != "success" }

    /// Reaction color per kind — exact RGB from the web's `exciteColor()`.
    private var exciteColor: Color {
        switch stimulusKind {
        case "success": return Color(.sRGB, red: 0.0, green: 1.0, blue: 0.62) // jade
        case "failure": return Color(.sRGB, red: 1.0, green: 0.0, blue: 0.40) // rose
        default: return Color(.sRGB, red: 1.0, green: 0.45, blue: 0.05)       // ember
        }
    }

    private func smoothstep01(_ x: Double) -> Double {
        let t = min(max(x, 0), 1)
        return t * t * (3 - 2 * t)
    }
}
