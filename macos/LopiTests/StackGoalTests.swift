import XCTest
@testable import Lopi

/// Goal-pursuit core tests — the Swift port of `web/src/lib/stores/stackGoal.test.ts`,
/// same fixtures and assertions. Pure functions only: no store, no mock, no
/// timers. This is the acceptance bar for the `StackGoal.swift` port.
final class StackGoalTests: XCTestCase {

    // precedence mirrors lopi-core StopReason: goal_met > budget > no_progress
    // > max_chain_loops
    func testPrecedence() {
        XCTAssertEqual(precede(.noProgress, .budget), .budget, "budget outranks no_progress")
        XCTAssertEqual(precede(.budget, .noProgress), .budget, "precede is order-independent")
        XCTAssertEqual(precede(.maxChainLoops, .goalMet), .goalMet, "goal_met outranks everything")
        XCTAssertEqual(precede(.noProgress, .maxChainLoops), .noProgress, "no_progress outranks the ceiling backstop")
        XCTAssertEqual(precede(.budget, .budget), .budget, "same reason is idempotent")
    }

    func testSuccessPredicate() {
        XCTAssertTrue(isSuccessStop(.goalMet), "goal_met is a successful stop")
        for r: StackStopReason in [.budget, .noProgress, .maxChainLoops] {
            XCTAssertFalse(isSuccessStop(r), "\(r.rawValue) is not a successful stop")
        }
    }

    func testEveryReasonHasDistinctNonEmptyLabel() {
        let labels = StackStopReason.allCases.map(stackStopLabel)
        XCTAssertTrue(labels.allSatisfy { !$0.isEmpty }, "every stop reason renders a non-empty label")
        XCTAssertEqual(Set(labels).count, labels.count, "each stop reason renders a distinct label")
    }

    // decideAfterMiss: keep re-running until a cap trips
    func testDecideAfterMiss() {
        XCTAssertEqual(
            decideAfterMiss(GoalPursuit(chainRun: 1, maxChainLoops: 3, noGainStreak: 0, noProgressLimit: 3)),
            .rerun, "below every cap → re-run the chain")
        XCTAssertEqual(
            decideAfterMiss(GoalPursuit(chainRun: 3, maxChainLoops: 3, noGainStreak: 0, noProgressLimit: 3)),
            .stop(reason: .maxChainLoops), "reaching the chain-loop ceiling stops with max_chain_loops")
        XCTAssertEqual(
            decideAfterMiss(GoalPursuit(chainRun: 5, maxChainLoops: 0, noGainStreak: 2, noProgressLimit: 3)),
            .rerun, "an infinite (0) ceiling never trips max_chain_loops on its own")
        XCTAssertEqual(
            decideAfterMiss(GoalPursuit(chainRun: 2, maxChainLoops: 0, noGainStreak: 3, noProgressLimit: 3)),
            .stop(reason: .noProgress), "a stalled infinite-ceiling stack stops with no_progress")
        XCTAssertEqual(
            decideAfterMiss(GoalPursuit(chainRun: 3, maxChainLoops: 3, noGainStreak: 3, noProgressLimit: 3)),
            .stop(reason: .noProgress), "both caps trip → higher-precedence no_progress reported")
        XCTAssertEqual(
            decideAfterMiss(GoalPursuit(chainRun: 5, maxChainLoops: 0, noGainStreak: 5, noProgressLimit: 0)),
            .rerun, "noProgressLimit 0 disables the no-progress detector")
    }

    // foldGain: reuse A3's gain margin at stack scope
    func testFoldGain() {
        let start = GainState(best: nil, streak: 0)
        let first = foldGain(start, 0.5)
        XCTAssertEqual(first, GainState(best: 0.5, streak: 0), "first observed score seeds best, zero streak")

        let gained = foldGain(first, 0.5 + STACK_GAIN_MARGIN)
        XCTAssertEqual(gained, GainState(best: 0.5 + STACK_GAIN_MARGIN, streak: 0), "beating best by the margin resets streak")

        let stalled = foldGain(gained, gained.best!)
        XCTAssertEqual(stalled.streak, 1, "a score that only ties the best increments the streak")
        XCTAssertEqual(stalled.best, gained.best, "a non-gaining chain-run leaves best untouched")

        let regressed = foldGain(stalled, 0.1)
        XCTAssertEqual(regressed.streak, 2, "a regression increments the streak again")

        let unobservable = foldGain(regressed, nil)
        XCTAssertEqual(unobservable, regressed, "an unobservable (nil) score changes neither best nor streak")
    }
}
