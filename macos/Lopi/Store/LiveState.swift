import Foundation
import SwiftUI

/// Live per-task cognition state, assembled from the `/ws` event stream. One
/// entry exists per task the server has told us about this session; `active`
/// flips false on completion/cancellation so the grid can fade it out.
struct LiveAgent: Identifiable, Hashable {
    let id: String // task id
    var goal: String
    var phase: String
    var attempt: Int
    var branch: String?

    // Latest turn_metrics (cognition).
    var pressure: Double = 0 // context-window fill, 0...1
    var activity: Double = 0 // generation intensity, 0...1
    var tokensPerSec: Double = 0
    var costUsd: Double = 0

    // Latest score_updated.
    var testPassRate: Double?
    var lintErrors: Int?
    var diffLines: Int?

    // Latest verifier_verdict.
    var verdictPassed: Bool?
    var verdictConfidence: Double?

    var active: Bool = true
    var lastUpdate: Date = .now

    /// Accent color encoding the current phase — shared by orbs, rings, glows.
    var accent: Color { PhaseStyle.color(phase) }
}

/// One row in the live event ticker.
struct FeedItem: Identifiable, Hashable {
    let id = UUID()
    let kind: Kind
    let title: String
    let detail: String
    let at: Date

    enum Kind: Hashable {
        case queued, started, status, score, verdictPass, verdictFail
        case completed, cancelled, budget, log, warn, error

        var icon: String {
            switch self {
            case .queued: return "tray.and.arrow.down"
            case .started: return "play.circle"
            case .status: return "arrow.triangle.2.circlepath"
            case .score: return "gauge.medium"
            case .verdictPass: return "checkmark.seal"
            case .verdictFail: return "xmark.seal"
            case .completed: return "checkmark.circle.fill"
            case .cancelled: return "minus.circle"
            case .budget: return "dollarsign.circle"
            case .log: return "text.alignleft"
            case .warn: return "exclamationmark.triangle"
            case .error: return "exclamationmark.octagon"
            }
        }

        var color: Color {
            switch self {
            case .completed, .verdictPass: return Konjo.ok
            case .cancelled, .status, .log: return Konjo.fgDim
            case .verdictFail, .error: return Konjo.err
            case .budget, .warn: return Konjo.warn
            case .queued, .started, .score: return Konjo.konjo2
            }
        }
    }
}

/// Most recent budget breach, surfaced as a pulsing banner on the dashboard.
struct BudgetBreach: Equatable {
    let scope: String
    let limitUsd: Double
    let burnedUsd: Double
    let at: Date
}

/// Maps a lopi phase/status string to its Konjo accent + whether it's "thinking".
enum PhaseStyle {
    static func color(_ phase: String) -> Color {
        switch phase.lowercased() {
        case "success", "done", "completed": return Konjo.ok
        case "failed", "rolledback", "rolled_back", "cancelled": return Konjo.err
        case "testing", "scoring", "retrying", "verifying": return Konjo.warn
        case "queued", "pending": return Konjo.fgMute
        default: return Konjo.konjo // planning / implementing / active
        }
    }

    /// Phases where the agent is actively computing — drives pulsing motion.
    static func isActive(_ phase: String) -> Bool {
        !["success", "done", "completed", "failed", "queued", "pending", "cancelled",
          "rolledback", "rolled_back"].contains(phase.lowercased())
    }
}
