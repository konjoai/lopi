import SwiftUI

/// The living-orb status description for one session — the macOS mirror of the
/// web `orbState.ts`, kept 1:1 so both UIs drive the orb from the same ORB STATE
/// MAP. The phase→color source of truth stays `PhaseStyle.color`; this layers the
/// terminal/blocked overrides and the motion parameters on top.
///
/// NOTE: written to mirror the verified web implementation; this macOS target was
/// not compiled in the authoring environment (Linux) — build on the M3.
struct ForgeOrbState: Equatable {
    /// State hue.
    var glowColor: Color
    /// Spin rate, baseline 1; 0 only on `.hardStop`.
    var spinSpeed: Double
    /// Pulse-frequency multiplier, baseline 1.
    var pulseRate: Double
    /// Aura / rim brightness, ~0.2 (idle) … ~1.4 (success bloom).
    var glowIntensity: Double
    /// Surface displacement intensity, 0…1.
    var turbulence: Double
    /// A named motion flourish layered on top.
    var special: Special

    /// Non-color motion flourishes the renderer special-cases.
    enum Special: Equatable {
        case none
        case kryptonite // jade halo pulses then settles (success)
        case hardStop // no spin, hard steady rim (failed / cancelled)
        case reverseSpin // agitated reverse rotation (rolling back)
        case stutter // jittery spin (rate-limited / retry)
        case attentionPulse // gentle pulse while awaiting the user
    }

    /// The calm orb shown when a pane holds no session.
    static let idle = ForgeOrbState(
        glowColor: Konjo.ice, spinSpeed: 0.25, pulseRate: 0.5,
        glowIntensity: 0.25, turbulence: 0.1, special: .none
    )
}

enum OrbStateMap {
    /// Compute the orb state for a session. `awaiting` is the externally-derived
    /// permission-waiting flag; pass `false` if unknown. Mirrors the web
    /// `computeOrbState`, including the override precedence (terminal/blocked win
    /// over the plain phase coloring) and the "only hardStop fully stops" rule.
    static func compute(_ agent: LiveAgent?, awaiting: Bool = false) -> ForgeOrbState {
        guard let agent else { return .idle }
        let phase = agent.phase.lowercased()

        if phase.contains("rolledback") || phase.contains("rolled_back") {
            return ForgeOrbState(glowColor: Konjo.ember, spinSpeed: 1.4, pulseRate: 1.4,
                                 glowIntensity: 1.0, turbulence: 0.8, special: .reverseSpin)
        }
        if isFailed(phase) {
            return ForgeOrbState(glowColor: Konjo.rose, spinSpeed: 0, pulseRate: 0,
                                 glowIntensity: 1.0, turbulence: 0, special: .hardStop)
        }
        if phase == "cancelled" {
            return ForgeOrbState(glowColor: Konjo.roseMuted, spinSpeed: 0, pulseRate: 0,
                                 glowIntensity: 0.6, turbulence: 0, special: .hardStop)
        }
        if isSuccess(phase) {
            return ForgeOrbState(glowColor: Konjo.jade, spinSpeed: 0.35, pulseRate: 0.8,
                                 glowIntensity: 1.4, turbulence: 0.2, special: .kryptonite)
        }
        if awaiting || agent.awaitingApproval {
            return ForgeOrbState(glowColor: Konjo.sun, spinSpeed: 0.45, pulseRate: 0.7,
                                 glowIntensity: 0.9, turbulence: 0.25, special: .attentionPulse)
        }
        if agent.throttled {
            return ForgeOrbState(glowColor: Konjo.flame, spinSpeed: 0.9, pulseRate: 1.2,
                                 glowIntensity: 0.9, turbulence: 0.4, special: .stutter)
        }
        if phase == "queued" || phase == "pending" {
            return ForgeOrbState(glowColor: Konjo.iceDeep, spinSpeed: 0.5, pulseRate: 0.6,
                                 glowIntensity: 0.4, turbulence: 0.15, special: .none)
        }
        return running(agent, phase: phase)
    }

    private static func running(_ agent: LiveAgent, phase: String) -> ForgeOrbState {
        let act = min(max(agent.activity, 0), 1)
        if let claude = agent.claudePhase?.lowercased(), claude.contains("pr") {
            return ForgeOrbState(glowColor: Konjo.mint, spinSpeed: 1.4, pulseRate: 1.2,
                                 glowIntensity: 1.0, turbulence: 0.4, special: .none)
        }
        switch phase {
        case "implementing", "implementation", "coding", "building":
            return ForgeOrbState(glowColor: Konjo.plasma, spinSpeed: 1.6 + act, pulseRate: 1.4,
                                 glowIntensity: 1.2, turbulence: 0.9, special: .none)
        case "testing":
            return ForgeOrbState(glowColor: Konjo.violet, spinSpeed: 1.3, pulseRate: 1.3,
                                 glowIntensity: 0.95, turbulence: 0.5, special: .none)
        case "scoring", "verifying":
            return ForgeOrbState(glowColor: Konjo.violetBright, spinSpeed: 1.1, pulseRate: 1.2,
                                 glowIntensity: 0.95, turbulence: 0.35, special: .none)
        case "planning", "discovery":
            return ForgeOrbState(glowColor: Konjo.ice, spinSpeed: 0.9 + act * 0.6, pulseRate: 1.0,
                                 glowIntensity: 0.8, turbulence: 0.3, special: .none)
        default: // boot / active
            return ForgeOrbState(glowColor: Konjo.ice, spinSpeed: 0.6, pulseRate: 0.8,
                                 glowIntensity: 0.55, turbulence: 0.2, special: .none)
        }
    }

    private static func isSuccess(_ phase: String) -> Bool {
        ["success", "done", "completed", "conclusion"].contains(phase) || phase.contains("success")
    }

    private static func isFailed(_ phase: String) -> Bool {
        phase.contains("failed") || phase.contains("error")
    }
}
