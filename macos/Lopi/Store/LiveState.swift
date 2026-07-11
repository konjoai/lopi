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

    // stream-json pane inputs (Phase 1 event spine).
    var outputTokens: Int = 0 // cumulative output tokens this run (token_delta)
    var inputTokens: Int = 0 // input tokens for the current turn (token_delta)
    var cacheReadTokens: Int = 0 // cache-read tokens for the current turn (token_delta)
    var numTurns: Int = 0 // turns from the terminal result (cost)
    var sessionId: String? // CLI session UUID for --resume (cost)
    var claudePhase: String? // Claude's own phase label, e.g. "requesting" (phase)
    var lastTool: String? // most recent tool name (tool_call)
    var toolCalls: Int = 0 // count of tool calls this run (tool_call)
    var throttled: Bool = false // a rate_limit_event was seen (api_retry)
    var utilization: Double = 0 // 0...1 window utilization from the last api_retry

    // Latest score_updated.
    var testPassRate: Double?
    var lintErrors: Int?
    var diffLines: Int?

    // Latest verifier_verdict.
    var verdictPassed: Bool?
    var verdictConfidence: Double?

    var active: Bool = true
    var lastUpdate: Date = .now

    /// Recent log lines for this task — feeds the pane's log strip (web parity).
    var logTail: [AgentLog] = []

    /// Last stimulus that should make the orb react, and what kind it was
    /// (request → ember, success → jade, failure → rose). Bump `stimulus` to
    /// `.now` to fire a reaction.
    var stimulus: Date = .distantPast
    var stimulusKind: String = "request"

    /// Phase 11 — set while the agent is paused at the plan approval gate.
    var awaitingApproval: Bool = false
    var planSteps: [String] = []
    var planText: String = ""

    /// Accent color encoding the current phase — shared by orbs, rings, glows.
    var accent: Color { PhaseStyle.color(phase) }
}

/// A single log line scoped to one agent (level + message).
struct AgentLog: Hashable {
    let level: String
    let text: String
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
    /// Maps a phase to its Konjo spectrum hue, 1:1 with the web Forge palette:
    /// cyan planning → ember implementing → gold testing → jade conclusion.
    static func color(_ phase: String) -> Color {
        switch phase.lowercased() {
        case "success", "done", "completed", "conclusion": return Konjo.jade
        case "failed", "rolledback", "rolled_back": return Konjo.rose
        case "cancelled": return Konjo.roseMuted
        // K-collision: Testing is violet, not yellow (sun is reserved for the
        // awaiting-user state); Scoring/verifying steps up to bright violet.
        case "testing": return Konjo.violet
        case "scoring", "verifying": return Konjo.violetBright
        case "retrying": return Konjo.flame // rate-limited / retry
        case "openingpr", "opening_pr", "pr": return Konjo.mint
        case "implementing", "implementation", "coding", "building": return Konjo.ember
        case "queued", "pending": return Konjo.fgMute
        default: return Konjo.ice // planning / discovery / boot / active
        }
    }

    /// Phases where the agent is actively computing — drives pulsing motion.
    static func isActive(_ phase: String) -> Bool {
        !["success", "done", "completed", "failed", "queued", "pending", "cancelled",
          "rolledback", "rolled_back"].contains(phase.lowercased())
    }
}

/// Lifecycle bucket a task's phase/status falls into for the Dashboard fleet
/// tiles — the Swift mirror of web's `dbStatusToUiStatus` (Fix-2 F3/F4). The
/// tiles count the live session map through this instead of trusting the WS
/// `pool_stats` event, whose counters are a single pool's and undercount in
/// multi-repo mode (Verify-2 F10).
enum FleetBucket: Hashable {
    /// In flight — planning, implementing, testing, retrying, awaiting approval.
    case running
    /// Accepted but not yet started.
    case queued
    /// Reached a successful terminal state.
    case succeeded
    /// Reached a failed terminal state (failed / rolled back / conflict).
    case failed
    /// Cancelled — terminal, but counted in none of the four fleet tiles, exactly
    /// as web excludes it from running/queued/completed/failed.
    case cancelled

    /// Bucket a phase/status token in whatever casing the wire or
    /// `TaskStatusLabel` produced. An unknown or brand-new token reads as
    /// `running` — in flight rather than silently terminal — matching web's
    /// `default` arm.
    static func of(_ phase: String) -> FleetBucket {
        switch phase.lowercased() {
        case "queued", "pending": return .queued
        case "success", "done", "completed", "conclusion": return .succeeded
        case "cancelled": return .cancelled
        case "failed", "rolled_back", "rolledback", "conflict", "unknown": return .failed
        default: return .running
        }
    }
}
