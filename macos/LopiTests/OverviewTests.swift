import XCTest
@testable import Lopi

/// Overview nav section — Swift mirror of web's `overview.test.ts`. Covers the
/// pure projection (`overviewRows`/`filterRows`/`filterCounts`/`formatElapsed`)
/// plus the two `AppModel` data-plumbing additions (`score` composite,
/// snapshot-hydrated `startedAt`) that feed it.
@MainActor
final class OverviewTests: XCTestCase {
    private func agent(_ id: String, phase: String, startedAt: Date = .now, cost: Double = 0) -> LiveAgent {
        var a = LiveAgent(id: id, goal: "goal \(id)", phase: phase, attempt: 0)
        a.startedAt = startedAt
        a.costUsd = cost
        return a
    }

    // MARK: overviewRows — sort order

    func testOverviewRowsSortsByLifecycleRankThenMostRecentFirst() {
        let now = Date()
        let agents = [
            agent("done-old", phase: "completed", startedAt: now.addingTimeInterval(-500)),
            agent("running-old", phase: "implementing", startedAt: now.addingTimeInterval(-300)),
            agent("running-new", phase: "planning", startedAt: now.addingTimeInterval(-10)),
            agent("queued", phase: "queued", startedAt: now.addingTimeInterval(-5)),
            agent("failed", phase: "failed", startedAt: now.addingTimeInterval(-1)),
        ]
        let rows = overviewRows(agents, now: now)
        XCTAssertEqual(rows.map(\.id), ["running-old", "running-new", "queued", "done-old", "failed"],
                        "running < queued < done < failed, most-recently-started first within a rank")
    }

    func testOverviewRowsComputesElapsedFromStartedAt() {
        let now = Date()
        let rows = overviewRows([agent("a", phase: "running", startedAt: now.addingTimeInterval(-90))], now: now)
        XCTAssertEqual(rows[0].elapsedMs, 90_000, accuracy: 1)
    }

    func testOverviewRowsCarriesAwaitingFlag() {
        var a = agent("a", phase: "AwaitingPlanApproval")
        a.awaitingApproval = true
        let rows = overviewRows([a])
        XCTAssertTrue(rows[0].awaiting)
    }

    // MARK: filterRows / filterCounts

    func testFilterRowsAndCounts() {
        let rows = overviewRows([
            agent("r1", phase: "running"),
            agent("r2", phase: "implementing"),
            agent("q1", phase: "queued"),
            agent("d1", phase: "completed"),
            agent("f1", phase: "failed"),
            agent("c1", phase: "cancelled"),
        ])
        let counts = filterCounts(rows)
        XCTAssertEqual(counts[.all], 6)
        XCTAssertEqual(counts[.running], 2)
        XCTAssertEqual(counts[.queued], 1)
        XCTAssertEqual(counts[.done], 1)
        XCTAssertEqual(counts[.deadLetter], 2, "dead-letter folds failed + cancelled")

        XCTAssertEqual(filterRows(rows, .all).count, 6)
        XCTAssertEqual(filterRows(rows, .running).count, 2)
        XCTAssertEqual(Set(filterRows(rows, .deadLetter).map(\.id)), ["f1", "c1"])
    }

    // MARK: formatElapsed

    func testFormatElapsed() {
        XCTAssertEqual(formatElapsed(0), "0s")
        XCTAssertEqual(formatElapsed(45_000), "45s")
        XCTAssertEqual(formatElapsed(60_000), "1m 0s")
        XCTAssertEqual(formatElapsed(90_000), "1m 30s")
        XCTAssertEqual(formatElapsed(90 * 60_000 + 12_000), "90m 12s", "no hour unit even past 60 minutes")
    }

    // MARK: score composite — AppModel+Live.swift's `.scoreUpdated` case

    func testScoreUpdatedComputesCompositeFormula() {
        let model = AppModel()
        model.liveAgents["a"] = agent("a", phase: "testing")
        model.ingest(.scoreUpdated(taskId: "a", testPassRate: 1.0, lintErrors: 0, diffLines: 40))
        XCTAssertEqual(model.liveAgents["a"]?.score ?? -1, 0.85, accuracy: 1e-9)

        model.liveAgents["b"] = agent("b", phase: "testing")
        model.ingest(.scoreUpdated(taskId: "b", testPassRate: 0.5, lintErrors: 10, diffLines: 5))
        // clamp01(0.5*0.85 - min(10/50, 0.15)) = clamp01(0.425 - 0.15) = 0.275
        XCTAssertEqual(model.liveAgents["b"]?.score ?? -1, 0.275, accuracy: 1e-9)

        model.liveAgents["c"] = agent("c", phase: "testing")
        model.ingest(.scoreUpdated(taskId: "c", testPassRate: 0, lintErrors: 200, diffLines: 0))
        XCTAssertEqual(model.liveAgents["c"]?.score ?? -1, 0, accuracy: 1e-9, "composite floors at 0")
    }

    // MARK: budget breach history — AppModel+Live.swift's `.budgetExceeded` case

    func testBudgetBreachesDedupsAndCapsAtFive() {
        let model = AppModel()
        for i in 0..<6 {
            model.ingest(.budgetExceeded(taskId: "t\(i)", scope: "task", limitUsd: 1, burnedUsd: Double(i)))
        }
        XCTAssertEqual(model.budgetBreaches.count, 5, "caps at 5 entries")
        XCTAssertEqual(model.budgetBreaches.map(\.taskId), ["t1", "t2", "t3", "t4", "t5"],
                       "oldest is dropped, newest is last")
        XCTAssertEqual(model.lastBudget?.taskId, "t5")
    }

    func testBudgetBreachesRepeatForSameScopeAndTaskMovesToEnd() {
        let model = AppModel()
        model.ingest(.budgetExceeded(taskId: "t1", scope: "task", limitUsd: 1, burnedUsd: 1))
        model.ingest(.budgetExceeded(taskId: "t2", scope: "task", limitUsd: 1, burnedUsd: 1))
        model.ingest(.budgetExceeded(taskId: "t1", scope: "task", limitUsd: 1, burnedUsd: 2))
        XCTAssertEqual(model.budgetBreaches.count, 2, "a repeat for the same (scope, taskId) replaces, not appends")
        XCTAssertEqual(model.budgetBreaches.map(\.taskId), ["t2", "t1"], "the repeated entry moves to the end")
        XCTAssertEqual(model.budgetBreaches.last?.burnedUsd ?? -1, 2, accuracy: 1e-9, "the entry carries the newer burned amount")
    }

    func testBudgetBreachesDistinctScopesDoNotDedupe() {
        let model = AppModel()
        model.ingest(.budgetExceeded(taskId: "t1", scope: "task", limitUsd: 1, burnedUsd: 1))
        model.ingest(.budgetExceeded(taskId: "t1", scope: "fleet", limitUsd: 5, burnedUsd: 5))
        XCTAssertEqual(model.budgetBreaches.count, 2, "same taskId but different scope is a distinct entry")
    }

    // MARK: startedAt hydration from the snapshot's `created_at`

    func testSnapshotHydratesStartedAtForNewTasks() {
        let model = AppModel()
        model.hydrateSnapshotTasks([
            ["id": "a", "goal": "g", "status": "queued", "created_at": "2026-01-01T00:00:00Z"],
        ])
        let expected = ISO8601DateFormatter().date(from: "2026-01-01T00:00:00Z")
        XCTAssertEqual(model.liveAgents["a"]?.startedAt, expected)
    }

    func testSnapshotDoesNotClobberLiveStartedAtOnReconnect() {
        let model = AppModel()
        let firstSight = Date().addingTimeInterval(-1000)
        model.liveAgents["a"] = agent("a", phase: "implementing", startedAt: firstSight)
        model.hydrateSnapshotTasks([
            ["id": "a", "goal": "g", "status": "implementing", "created_at": "2020-01-01T00:00:00Z"],
        ])
        XCTAssertEqual(model.liveAgents["a"]?.startedAt, firstSight,
                        "an already-seen task keeps its own first-sight startedAt")
    }
}
