import Foundation

// B1 — goal-directed stacks: the pure decision core the client sequencer
// (`StackRun`) drives. Pure port of `stores/stackGoal.ts`. The stop-reason
// vocabulary mirrors the backend A3 termination (`stop_reason.rs`) at *chain*
// scope, with loop-scope `max_iterations` re-cast as `max_chain_loops`, and the
// same precedence `goal_met > budget > no_progress > max_chain_loops`.
// Foundation only.

/// A chain-scope stop reason. Wire strings match `StopReason::as_str`.
public enum StackStopReason: String, Equatable, CaseIterable {
    case goalMet = "goal_met"
    case budget = "budget"
    case noProgress = "no_progress"
    case maxChainLoops = "max_chain_loops"
}

/// Precedence rank, higher wins — mirrors `StopReason::rank`.
private let RANK: [StackStopReason: Int] = [
    .maxChainLoops: 0,
    .noProgress: 1,
    .budget: 2,
    .goalMet: 3
]

/// The higher-precedence of two reasons — the one that "wins" when both trip in
/// the same chain-run. Mirrors `StopReason::precede`.
public func precede(_ a: StackStopReason, _ b: StackStopReason) -> StackStopReason {
    (RANK[b] ?? 0) > (RANK[a] ?? 0) ? b : a
}

/// Whether a stop reason represents a *successful* termination.
public func isSuccessStop(_ reason: StackStopReason) -> Bool { reason == .goalMet }

/// A short, human-readable line for the recorded stop reason.
public func stackStopLabel(_ reason: StackStopReason) -> String {
    switch reason {
    case .goalMet:
        return "goal met — stack acceptance passed"
    case .budget:
        return "stopped — stack budget exhausted"
    case .noProgress:
        return "stopped — no progress across chain re-runs"
    case .maxChainLoops:
        return "stopped — reached the chain-loop ceiling without meeting the goal"
    }
}

/// The margin a chain-run's stack-eval score must beat the best-so-far by to
/// count as progress — mirrors A3's `GainRule.margin`.
public let STACK_GAIN_MARGIN = 0.01

/// The live goal-pursuit counters the sequencer threads across chain-runs.
public struct GoalPursuit {
    public var chainRun: Int
    public var maxChainLoops: Int
    public var noGainStreak: Int
    public var noProgressLimit: Int

    public init(chainRun: Int, maxChainLoops: Int, noGainStreak: Int, noProgressLimit: Int) {
        self.chainRun = chainRun
        self.maxChainLoops = maxChainLoops
        self.noGainStreak = noGainStreak
        self.noProgressLimit = noProgressLimit
    }
}

/// What the sequencer should do after a chain-run whose acceptance did not pass.
public enum GoalDecision: Equatable {
    case rerun
    case stop(reason: StackStopReason)
}

/// Decide what to do after a chain-run whose stack acceptance did **not** pass
/// (`goal_met` is handled by the caller, before this is reached). Both caps are
/// checked and the higher-precedence one is reported. `budget` is intentionally
/// never tripped here (no observable client-side stack-token meter), but stays
/// in the precedence for when a real meter lands.
public func decideAfterMiss(_ p: GoalPursuit) -> GoalDecision {
    var tripped: [StackStopReason] = []
    if p.maxChainLoops != 0 && p.chainRun >= p.maxChainLoops { tripped.append(.maxChainLoops) }
    if p.noProgressLimit != 0 && p.noGainStreak >= p.noProgressLimit { tripped.append(.noProgress) }
    if tripped.isEmpty { return .rerun }
    return .stop(reason: tripped.reduce(tripped[0], precede))
}

/// The best-score + no-gain-streak carried between chain-runs.
public struct GainState: Equatable {
    public var best: Double?
    public var streak: Int

    public init(best: Double? = nil, streak: Int) {
        self.best = best
        self.streak = streak
    }
}

/// Fold a completed chain-run's stack-eval score into the no-gain streak. A
/// score at or above `best + margin` is progress (resets streak, raises best);
/// anything less increments the streak. `nil` (no observable scalar) leaves both
/// unchanged — an unobservable result is neither progress nor a stall.
public func foldGain(_ prev: GainState, _ score: Double?) -> GainState {
    guard let score else { return prev }
    if prev.best == nil || score >= (prev.best ?? 0) + STACK_GAIN_MARGIN {
        return GainState(best: score, streak: 0)
    }
    return GainState(best: prev.best, streak: prev.streak + 1)
}
