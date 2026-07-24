import XCTest
@testable import Lopi

/// Fix-3 — macOS stats/cost parity (Verify-2 F9/F10 + the F6 port).
///
/// Locks the three data-path corrections that port Fix-2's web fix to the Swift
/// client: the fleet tiles count the live session map (not the per-pool WS
/// event), and per-task cost hydrates from the snapshot so Budget SPENT isn't $0.
@MainActor
final class StatsParityTests: XCTestCase {
    // MARK: FleetBucket — Swift mirror of web's `dbStatusToUiStatus`

    func testFleetBucketMirrorsWebStatusMapping() {
        // Queued.
        XCTAssertEqual(FleetBucket.of("queued"), .queued)
        XCTAssertEqual(FleetBucket.of("Queued"), .queued)
        XCTAssertEqual(FleetBucket.of("pending"), .queued)
        // In flight — every working phase, and any unknown/new token, read running.
        for phase in ["running", "planning", "implementing", "testing", "scoring",
                      "verifying", "retrying", "AwaitingPlanApproval", "brand_new_phase"] {
            XCTAssertEqual(FleetBucket.of(phase), .running, "\(phase) should bucket running")
        }
        // Succeeded.
        for phase in ["success", "Success", "done", "completed", "conclusion"] {
            XCTAssertEqual(FleetBucket.of(phase), .succeeded, "\(phase) should bucket succeeded")
        }
        // Failed — including the enum spellings `TaskStatusLabel` emits.
        for phase in ["failed", "Failed", "rolled_back", "RolledBack", "conflict", "unknown"] {
            XCTAssertEqual(FleetBucket.of(phase), .failed, "\(phase) should bucket failed")
        }
        // Cancelled is terminal but excluded from all four tiles (web parity).
        XCTAssertEqual(FleetBucket.of("cancelled"), .cancelled)
    }

    // MARK: Fleet counts — the multi-repo undercount the WS pool event caused

    func testFleetCountsReadFromSessionMapNotPoolEvent() {
        let model = AppModel()
        // A mixed batch spanning multiple repos — the exact shape a single pool's
        // counter would undercount (Verify-2 F10: RUNNING 1 of 2, SUCCEEDED 1 of 3).
        model.liveAgents = [
            "a": agent("a", phase: "implementing"),
            "b": agent("b", phase: "planning"),
            "c": agent("c", phase: "queued"),
            "d": agent("d", phase: "Success"),
            "e": agent("e", phase: "success"),
            "f": agent("f", phase: "done"),
            "g": agent("g", phase: "Failed"),
            "h": agent("h", phase: "cancelled"),
        ]
        XCTAssertEqual(model.runningCount, 2, "two agents in flight across repos")
        XCTAssertEqual(model.queuedCount, 1)
        XCTAssertEqual(model.succeededCount, 3, "three succeeded, not one pool's view")
        XCTAssertEqual(model.failedCount, 1)
        // Cancelled counts toward none of the tiles.
        XCTAssertEqual(model.runningCount + model.queuedCount
            + model.succeededCount + model.failedCount, 7)
    }

    // MARK: F6 — per-task cost hydrates from the snapshot

    func testSnapshotHydratesPerTaskCost() {
        let model = AppModel()
        model.hydrateSnapshotTasks([
            ["id": "a", "goal": "fix foo", "status": "success", "cost": 0.10],
            ["id": "b", "goal": "add bar", "status": "running", "cost": 0.05],
            ["id": "c", "goal": "no cost field", "status": "queued"],
        ])
        XCTAssertEqual(model.liveAgents["a"]?.costUsd ?? 0, 0.10, accuracy: 1e-9)
        XCTAssertEqual(model.liveAgents["b"]?.costUsd ?? 0, 0.05, accuracy: 1e-9)
        XCTAssertEqual(model.liveAgents["c"]?.costUsd ?? -1, 0, accuracy: 1e-9,
                       "missing cost field defaults to 0, not a decode failure")
        // Budget SPENT is the sum of per-agent cost — real spend, not $0.
        let spent = model.liveAgents.values.reduce(0) { $0 + $1.costUsd }
        XCTAssertEqual(spent, 0.15, accuracy: 1e-9)
    }

    func testSnapshotDoesNotClobberLiveCostOnReconnect() {
        let model = AppModel()
        // A task already live this session, with cost accrued from `cost` events.
        var live = agent("a", phase: "implementing")
        live.costUsd = 0.42
        model.liveAgents["a"] = live
        // A reconnect snapshot carries a staler DB cost for the same id.
        model.hydrateSnapshotTasks([
            ["id": "a", "goal": "fix foo", "status": "implementing", "cost": 0.10],
        ])
        XCTAssertEqual(model.liveAgents["a"]?.costUsd ?? 0, 0.42, accuracy: 1e-9,
                       "an already-seen task keeps its live cost — snapshot only hydrates new ids")
    }

    // MARK: macOS-Web-Parity-5 — repo hydrates the same way cost does

    func testTaskStartedEventSetsRepo() {
        let model = AppModel()
        model.ingest(.taskQueued(taskId: "a", goal: "fix foo", priority: ""))
        model.ingest(.taskStarted(taskId: "a", attempt: 1, branch: "lopi/a-attempt-1", repo: "/Users/dev/lopi"))
        XCTAssertEqual(model.liveAgents["a"]?.repo, "/Users/dev/lopi")
    }

    func testTaskStartedEmptyRepoDoesNotClobberExisting() {
        let model = AppModel()
        model.ingest(.taskQueued(taskId: "a", goal: "fix foo", priority: ""))
        model.ingest(.taskStarted(taskId: "a", attempt: 1, branch: "lopi/a-attempt-1", repo: "/Users/dev/lopi"))
        // A later attempt's event from an old server (or a genuinely empty
        // value) must not blank out the repo the first attempt already set —
        // mirrors `branch`'s own empty-string-as-absent convention.
        model.ingest(.taskStarted(taskId: "a", attempt: 2, branch: "lopi/a-attempt-2", repo: ""))
        XCTAssertEqual(model.liveAgents["a"]?.repo, "/Users/dev/lopi")
    }

    func testSnapshotHydratesPerTaskRepo() {
        let model = AppModel()
        model.hydrateSnapshotTasks([
            ["id": "a", "goal": "fix foo", "status": "running", "repo": "/Users/dev/lopi"],
            ["id": "b", "goal": "no repo yet", "status": "queued"],
        ])
        XCTAssertEqual(model.liveAgents["a"]?.repo, "/Users/dev/lopi")
        XCTAssertNil(model.liveAgents["b"]?.repo, "a task with no TaskStarted yet has no repo")
    }

    func testSnapshotDoesNotClobberLiveRepoOnReconnect() {
        let model = AppModel()
        var live = agent("a", phase: "implementing")
        live.repo = "/Users/dev/lopi"
        model.liveAgents["a"] = live
        // A reconnect snapshot carries the same repo for an already-known id —
        // the guard is "only hydrate new ids", same as cost, so this is a
        // no-op either way, but asserted explicitly for the same reason F6's
        // reconnect test is.
        model.hydrateSnapshotTasks([
            ["id": "a", "goal": "fix foo", "status": "implementing", "repo": "/Users/dev/other-repo"],
        ])
        XCTAssertEqual(model.liveAgents["a"]?.repo, "/Users/dev/lopi",
                       "an already-seen task keeps its live repo — snapshot only hydrates new ids")
    }

    // MARK: Helpers

    private func agent(_ id: String, phase: String) -> LiveAgent {
        LiveAgent(id: id, goal: id, phase: phase, attempt: 0)
    }
}
