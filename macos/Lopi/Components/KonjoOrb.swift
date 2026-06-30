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

    // ── Living-orb motion params (ORB STATE MAP, see ForgeOrbState). All have
    // defaults so existing call sites keep working; the pane passes a computed
    // state in. ──
    /// State hue; overrides the phase color when set so the orb takes the status color.
    var glowColor: Color? = nil
    /// Spin rate, baseline 1; 0 stops (only under `.hardStop`).
    var spinSpeed: Double = 1
    /// Pulse-frequency multiplier, baseline 1.
    var pulseRate: Double = 1
    /// Aura / rim brightness, ~0.2 (idle) … ~1.4 (success bloom).
    var glowIntensity: Double = 0.85
    /// Surface displacement intensity, 0…1, layered onto pressure.
    var turbulence: Double = 0.3
    /// A named motion flourish.
    var special: ForgeOrbState.Special = .none

    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    /// The moment this orb appeared. Animation time is measured from here so the
    /// value stays small — large absolute timestamps lose `Float` precision and
    /// have to be wrapped, which made the surface visibly restart. Mirrors the
    /// web orb, which accumulates time from zero.
    @State private var epoch = Date()

    /// How long a single stimulus burns (matches the web's EXCITE_DURATION_MS).
    private let exciteDuration: Double = 2.5
    /// The orb hue: the live state color when supplied, else the phase color.
    private var accent: Color { glowColor ?? PhaseStyle.color(phase) }

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
                        .color(exciteColor),
                        // Living-orb motion uniforms.
                        .float(Float(max(pulseRate, 0))),
                        .float(Float(min(max(glowIntensity, 0), 2))),
                        .float(Float(min(max(turbulence, 0), 1))),
                        .float(f.krypto)
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
        var krypto: Float
        var ox: CGFloat
        var oy: CGFloat
    }

    private func frame(at date: Date) -> Frame {
        let nowRef = date.timeIntervalSinceReferenceDate
        // Seconds since the orb appeared — small and monotonic, so the noise
        // field flows smoothly for the whole session instead of jumping when an
        // absolute-time counter wraps.
        let t = reduceMotion ? 0 : date.timeIntervalSince(epoch)

        // Excitement envelope — linear decay from the stimulus timestamp.
        let since = date.timeIntervalSince(stimulus)
        let excite = (!reduceMotion && since >= 0) ? max(0, 1 - since / exciteDuration) : 0

        // Rotation: a state-scaled base drift plus the stimulus spin-up. The ORB
        // STATE MAP's spinSpeed scales the rate; `.hardStop` freezes it,
        // `.reverseSpin` runs it backwards, `.stutter` jitters it. Only hardStop
        // ever fully stops.
        let rateScale = motionRateScale(nowRef)
        let baseRate = 0.22 * rateScale
        var spin = t * baseRate
        if !reduceMotion && since >= 0 {
            let a = min(max(since / exciteDuration, 0), 1)
            let inv = 1 - a
            let area = 0.5 - (inv * inv * inv - 0.5 * inv * inv * inv * inv)
            spin += baseRate * 1.8 * exciteDuration * area
        }

        // Kryptonite — a bright jade halo that pulses ~2–3× on arrival (success)
        // then settles to a low steady glow as the orb drifts down.
        let krypto = kryptoLevel(nowRef)

        // Shake: a faint front-loaded nudge (excite³) for requests/failures —
        // a reaction, not a rattle.
        let shakeAmp = shakes ? excite * excite * excite * Double(size) * 0.002 : 0
        return Frame(
            time: Float(t),
            spin: Float(spin.truncatingRemainder(dividingBy: 10 * .pi)),
            excite: Float(smoothstep01(excite)),
            krypto: Float(krypto),
            ox: CGFloat(sin(nowRef * 53.0) * shakeAmp),
            oy: CGFloat(cos(nowRef * 61.0) * shakeAmp)
        )
    }

    /// Spin-rate multiplier from `spinSpeed` + `special`. hardStop → 0, reverse →
    /// negative, stutter → jittered. reduce-motion already pauses the timeline.
    private func motionRateScale(_ nowRef: Double) -> Double {
        switch special {
        case .hardStop: return 0
        case .reverseSpin: return -abs(spinSpeed)
        case .stutter: return spinSpeed * (0.6 + 0.9 * ((sin(nowRef * 7.3) + 1) / 2))
        default: return spinSpeed
        }
    }

    /// Kryptonite envelope, 0…~1: a pulsing jade level while `special` is
    /// kryptonite, settling toward a low floor; 0 otherwise.
    private func kryptoLevel(_ nowRef: Double) -> Double {
        guard special == .kryptonite else { return 0 }
        if reduceMotion { return 0.4 }
        let since = max(0, nowRef.truncatingRemainder(dividingBy: 3600) - 0) // continuous clock
        let pulse = 0.6 + 0.4 * sin(since * 6.0)
        return max(0.25, pulse) * min(1, glowIntensity)
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
