import XCTest
@testable import Lopi
import LopiStacksKit

/// Overview board (kanban) tests — Swift port of
/// `web/src/lib/stores/stackOverview.test.ts`. Seeds panes (client-only Loop
/// Stacks) plus a synthetic live-agent map and checks
/// `buildStackOverviewCards` buckets each stack into the right lifecycle
/// column, resolves its representative goal/repo/branch, and formats its
/// meta line — the same shape `OverviewTests.swift` uses for the per-agent
/// rollup. Color assertions are deliberately omitted (no precedent in
/// `OverviewTests.swift` either) — `lifecycle`/`failed`/`metaRight` already
/// prove the color-selection branches without relying on `Color` equality.
@MainActor
final class StackOverviewTests: XCTestCase {

    private func card(_ id: String, _ goal: String? = nil, _ patch: (inout StackCard) -> Void = { _ in }) -> StackCard {
        var c = buildCard("\"\(goal ?? id)\"")
        c.id = id
        patch(&c)
        return c
    }
    private func pane(_ key: String, _ cards: [StackCard]) -> StackPaneState {
        StackPaneState(key: key, title: key, cards: cards, config: defaultStackConfig())
    }
    private func agent(_ id: String, phase: String, startedAt: Date = .now, cost: Double = 0) -> LiveAgent {
        var a = LiveAgent(id: id, goal: "g", phase: phase, attempt: 0)
        a.startedAt = startedAt
        a.costUsd = cost
        return a
    }

    // MARK: bare panes are excluded

    func testBarePaneNeverReachesBoard() {
        XCTAssertEqual(buildStackOverviewCards([pane("bare", [])], [:]).count, 0)
    }

    // MARK: queued — nothing has ever run

    func testQueuedWhenNothingHasRun() {
        let p = pane("s1", [card("a", "do the thing"), card("b", "then this") { $0.status = .queued }])
        let cards = buildStackOverviewCards([p], [:])
        XCTAssertEqual(cards[0].lifecycle, .queued, "no running/terminal cards -> queued")
        XCTAssertEqual(cards[0].metaRight, "queued")
        XCTAssertEqual(cards[0].loopCount, 2, "loop count matches card count")
    }

    // MARK: running — a card is mid-flight, agent phase not Testing

    func testRunningBucketsAsRunningNotTesting() {
        var running = card("r", "count files")
        running.status = .running
        running.taskId = "t1"
        let now = Date()
        let agents = ["t1": agent("t1", phase: "Implementation", startedAt: now.addingTimeInterval(-134), cost: 0.0041)]
        let cards = buildStackOverviewCards([pane("s2", [running])], agents, now: now)
        XCTAssertEqual(cards[0].lifecycle, .running, "Implementation phase buckets as running, not testing")
        XCTAssertEqual(cards[0].goal, "count files", "goal comes from the running card")
        XCTAssertEqual(cards[0].metaRight, "2m 14s · $0.0041", "running meta is elapsed + cost")
    }

    // MARK: testing — the running card's agent is in the Testing phase

    func testTestingPhaseBucketsAsTesting() {
        var running = card("t", "verify report")
        running.status = .running
        running.taskId = "t2"
        let now = Date()
        let agents = ["t2": agent("t2", phase: "Testing", startedAt: now.addingTimeInterval(-60), cost: 0.01)]
        let cards = buildStackOverviewCards([pane("s3", [running])], agents, now: now)
        XCTAssertEqual(cards[0].lifecycle, .testing, "Testing phase buckets as testing")
    }

    // MARK: done — every card terminal, all succeeded

    func testDoneWhenAllTerminalAndSucceeded() {
        var done = card("d1", "summarize")
        done.status = .done
        done.taskId = "a1"
        let agents = ["a1": agent("a1", phase: "done", cost: 0.0012)]
        let cards = buildStackOverviewCards([pane("s4", [done])], agents)
        XCTAssertEqual(cards[0].lifecycle, .done, "all-terminal cards -> done")
        XCTAssertFalse(cards[0].failed, "no blocked card means not failed")
        XCTAssertEqual(cards[0].metaRight, "$0.0012", "done meta is total cost")
    }

    // MARK: done + failed — a blocked card anywhere marks the whole stack failed

    func testBlockedCardMarksStackFailed() {
        var blocked = card("noop", "noop probe")
        blocked.status = .blocked
        blocked.blockReason = "error"
        let cards = buildStackOverviewCards([pane("s5", [blocked])], [:])
        XCTAssertEqual(cards[0].lifecycle, .done, "blocked-only stack still lands in done")
        XCTAssertTrue(cards[0].failed, "a blocked card marks the stack failed")
        XCTAssertEqual(cards[0].metaRight, "failed", "failed meta text overrides cost")
    }

    // MARK: loop dots — which loop pulses

    func testRunningLoopPulses() {
        var l1 = card("l1", "a"); l1.status = .done
        var l2 = card("l2", "b"); l2.status = .running; l2.taskId = "t3"
        let l3 = card("l3", "c") // idle
        let agents = ["t3": agent("t3", phase: "Implementation")]
        let cards = buildStackOverviewCards([pane("s6", [l1, l2, l3])], agents)
        XCTAssertTrue(cards[0].loops.contains { $0.pulsing }, "the running loop pulses")
        XCTAssertEqual(cards[0].loops.filter({ $0.pulsing }).count, 1, "only the running loop pulses")
    }

    // MARK: repo/branch fall back to the stack defaults when the card has none

    func testRepoAndBranchFallBackToPaneDefaults() {
        var p = pane("s7", [card("c1", "goal")])
        p.config.defaults.repo = "/Users/dev/lopi"
        p.config.defaults.branch = "feat/x"
        let cards = buildStackOverviewCards([p], [:])
        XCTAssertEqual(cards[0].repo, "lopi", "repo falls back to the pane default, basenamed")
        XCTAssertEqual(cards[0].branch, "feat/x", "branch falls back to the pane default")
    }

    // MARK: groupByLifecycle buckets in display order, and totalCost sums the map

    func testGroupByLifecycleAndTotalCost() {
        let cards = buildStackOverviewCards([pane("g1", [card("a", "x") { $0.status = .done }])], [:])
        let groups = groupByLifecycle(cards)
        XCTAssertEqual(groups[.done]?.map(\.key), ["g1"], "done card lands in the done bucket")
        XCTAssertEqual(groups[.queued]?.count, 0, "queued bucket empty when nothing queued")

        let total = totalCost(["a": agent("a", phase: "done", cost: 0.01), "b": agent("b", phase: "done", cost: 0.02)])
        XCTAssertEqual(total, 0.03, accuracy: 1e-9, "totalCost sums every agent in the map")
    }
}
