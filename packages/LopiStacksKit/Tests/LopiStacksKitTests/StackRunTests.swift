import XCTest
@testable import LopiStacksKit

/// Stack-run sequencer tests — the Swift port of
/// `web/src/lib/stores/stackRun.test.ts`. A deterministic in-memory mock stands
/// in for the backend (mirroring the web `mockBackend`): each card id's terminal
/// outcome is pre-decided and keyed by `client_ref` (== card id, per
/// `cardToTaskPayload`), which the mock echoes back as the effective task id, so
/// `waitForTerminal` resolves immediately. Pause/drain/bump are exercised from
/// *inside* the create-task hook — "the user clicked pause while this card was in
/// flight" — the only deterministic way to interrupt a synchronously-resolving
/// mock mid-run. The acceptance bar for `StackRun.swift`/`StackRunControls.swift`.
@MainActor
final class StackRunTests: XCTestCase {

    private let defaults = PaneDefaults(model: "m", effort: "e", repo: "r")

    /// A recorded create-task call.
    private struct Captured { let clientRef: String?; let payload: StackTaskPayload }

    /// The deterministic backend + the pane store it drives.
    @MainActor
    private final class Mock {
        let store: StackStore
        var outcomes: [String: TerminalStatus]
        var scores: [String: Double]
        var onCreate: ((String?) -> Void)?
        var bareTaskId = "bare-task-1"
        var scheduleCount = 0
        var captured: [Captured] = []

        init(store: StackStore, outcomes: [String: TerminalStatus], scores: [String: Double] = [:], onCreate: ((String?) -> Void)? = nil) {
            self.store = store
            self.outcomes = outcomes
            self.scores = scores
            self.onCreate = onCreate
        }

        func seams() -> StackRunSeams {
            StackRunSeams(
                panes: { [store] in store.panes },
                updateCard: { [store] key, id, mut in store.updateCardInPane(key, id, mut) },
                createTask: { [self] payload in
                    captured.append(Captured(clientRef: payload.options.clientRef, payload: payload))
                    onCreate?(payload.options.clientRef)
                    return payload.options.clientRef ?? bareTaskId
                },
                waitForTerminal: { [self] taskId in
                    if let o = outcomes[taskId] { return o }
                    // "still running" — never resolves, so later cards never launch.
                    return await withCheckedContinuation { (_: CheckedContinuation<TerminalStatus, Never>) in }
                },
                score: { [self] taskId in scores[taskId] },
                createSchedule: { [self] _, _, _, _, _ in scheduleCount += 1 },
                reorderPaneCards: { [store] key, from, to in store.reorderInPane(key, from, to) })
        }
    }

    // MARK: helpers

    private func card(_ id: String, _ goal: String? = nil) -> StackCard {
        var c = buildCard("\"\(goal ?? id)\""); c.id = id; return c
    }
    private func freshStore() -> StackStore {
        StackStore(panes: [
            StackPaneState(key: "s1", title: "stack one", cards: [], config: defaultStackConfig()),
            StackPaneState(key: "s2", title: "stack two", cards: [], config: defaultStackConfig())
        ])
    }
    /// Let the engine's detached advance Task run to (partial) completion.
    private func settle(_ rounds: Int = 400) async {
        for _ in 0..<rounds { await Task.yield() }
    }
    private func refs(_ mock: Mock) -> [String?] { mock.captured.map(\.clientRef) }

    // seedPane appends; the composer prepends in production, so to reproduce a
    // "newest-first" array [c,b,a] we insert a,b,c at the front instead.
    private func seedNewestFirst(_ store: StackStore, _ key: String, _ ids: [String]) {
        // ids given newest-first (as pane.cards is stored). Build by prepending
        // oldest→newest so the final array equals `ids`.
        for id in ids.reversed() { store.addToPane(key, card(id)) }
    }

    // MARK: tests

    func testOrderingBottomToTop() async {
        let store = freshStore()
        seedNewestFirst(store, "s1", ["c", "b", "a"]) // execution order a,b,c
        let mock = Mock(store: store, outcomes: ["a": .completed, "b": .completed, "c": .completed])
        let engine = StackRunEngine(seams: mock.seams())
        engine.runStack("s1", .run, defaults)
        await settle()
        XCTAssertEqual(refs(mock), ["a", "b", "c"], "3-card stack launches in execution order (bottom first)")
        XCTAssertEqual(engine.run(for: "s1")?.phase, .done, "fully-successful run ends done")
        XCTAssertEqual(engine.run(for: "s1")?.cursor, 3, "cursor lands past the end of the plan")
    }

    func testFailingCardHalts() async {
        let store = freshStore()
        seedNewestFirst(store, "s1", ["c", "b", "a"])
        let mock = Mock(store: store, outcomes: ["a": .completed, "b": .failed])
        let engine = StackRunEngine(seams: mock.seams())
        engine.runStack("s1", .run, defaults)
        await settle()
        XCTAssertEqual(refs(mock), ["a", "b"], "failing second card stops before the third launches")
        XCTAssertEqual(engine.run(for: "s1")?.phase, .error, "a failed card puts the run into error")
        XCTAssertTrue(engine.run(for: "s1")?.error?.contains("failed") ?? false, "the error names the outcome")
    }

    func testPauseThenResume() async {
        let store = freshStore()
        seedNewestFirst(store, "s1", ["c", "b", "a"])
        let mock = Mock(store: store, outcomes: ["a": .completed, "b": .completed, "c": .completed])
        var engine: StackRunEngine!
        mock.onCreate = { ref in if ref == "a" { engine.pauseStack("s1") } }
        engine = StackRunEngine(seams: mock.seams())
        engine.runStack("s1", .run, defaults)
        await settle()
        XCTAssertEqual(refs(mock), ["a"], "pausing while 'a' is in flight halts before 'b'")
        XCTAssertEqual(engine.run(for: "s1")?.phase, .paused, "phase is paused")
        XCTAssertEqual(engine.run(for: "s1")?.cursor, 1, "cursor advanced past the finished card")
        engine.resumeStack("s1", defaults)
        await settle()
        XCTAssertEqual(refs(mock), ["a", "b", "c"], "resuming continues the remaining cards")
        XCTAssertEqual(engine.run(for: "s1")?.phase, .done, "resuming through reaches done")
    }

    func testDrainNotResumable() async {
        let store = freshStore()
        seedNewestFirst(store, "s1", ["c", "b", "a"])
        let mock = Mock(store: store, outcomes: ["a": .completed, "b": .completed, "c": .completed])
        var engine: StackRunEngine!
        mock.onCreate = { ref in if ref == "a" { engine.drainStack("s1") } }
        engine = StackRunEngine(seams: mock.seams())
        engine.runStack("s1", .run, defaults)
        await settle()
        XCTAssertEqual(refs(mock), ["a"], "draining while 'a' is in flight lets it finish then stops")
        XCTAssertEqual(engine.run(for: "s1")?.phase, .done, "a drained run finalizes to done")
        engine.resumeStack("s1", defaults)
        await settle()
        XCTAssertEqual(refs(mock), ["a"], "resume is a no-op on a drained run")
    }

    func testBumpReflectsIntoPane() async {
        let store = freshStore()
        seedNewestFirst(store, "s1", ["d", "c", "b", "a"]) // execution order a,b,c,d
        let mock = Mock(store: store, outcomes: ["a": .completed, "b": .completed, "c": .completed, "d": .completed])
        var engine: StackRunEngine!
        mock.onCreate = { ref in if ref == "a" { engine.pauseStack("s1") } }
        engine = StackRunEngine(seams: mock.seams())
        engine.runStack("s1", .run, defaults)
        await settle()
        XCTAssertEqual(engine.run(for: "s1")?.phase, .paused, "sanity: paused after 'a'")
        let bumped = engine.bumpCard("s1", "d", .up)
        XCTAssertEqual(bumped, .ok(["a", "b", "d", "c"]), "the run's plan reflects the swap")
        XCTAssertEqual(store.pane(for: "s1")?.cards.map(\.id), ["c", "d", "b", "a"], "the pane's card order reflects the same swap")
        engine.resumeStack("s1", defaults)
        await settle()
        XCTAssertEqual(refs(mock), ["a", "b", "d", "c"], "the bumped order is what launches")
    }

    func testBumpRejectsIllegal() async {
        let store = freshStore()
        seedNewestFirst(store, "s1", ["c", "b", "a"])
        let mock = Mock(store: store, outcomes: [:])
        var engine: StackRunEngine!
        mock.onCreate = { ref in if ref == "a" { engine.pauseStack("s1") } }
        engine = StackRunEngine(seams: mock.seams())
        engine.runStack("s1", .run, defaults)
        await settle()
        if case .ok = engine.bumpCard("s1", "a", .down) { XCTFail("bumping the already-run card should be rejected") }
        if case .ok = engine.bumpCard("s2", "x", .up) { XCTFail("bumping in a pane with no active run should be rejected") }
    }

    func testScheduleStackHonest() async {
        let store = freshStore()
        seedNewestFirst(store, "s1", ["c", "b", "a"])
        let mock = Mock(store: store, outcomes: [:])
        let engine = StackRunEngine(seams: mock.seams())
        let result = await engine.scheduleStack("s1", "0 * * * *", defaults)
        XCTAssertTrue(result.ok, "scheduleStack succeeds")
        XCTAssertEqual(result.scheduledCardId, "a", "only the bottom (first-to-run) card is scheduled")
        XCTAssertEqual(result.skippedCardIds, ["b", "c"], "every other card reported as skipped")
        XCTAssertEqual(mock.scheduleCount, 1, "exactly one schedule is created, not one per card")
    }

    func testChainLoopTwice() async {
        let store = freshStore()
        seedNewestFirst(store, "s1", ["c", "b", "a"])
        store.updateStackConfig("s1") { $0.loopCount = 2 }
        let mock = Mock(store: store, outcomes: ["a": .completed, "b": .completed, "c": .completed])
        let engine = StackRunEngine(seams: mock.seams())
        engine.runStack("s1", .run, defaults)
        await settle()
        XCTAssertEqual(refs(mock), ["a", "b", "c", "a", "b", "c"], "×2 chain launches every card twice, same order")
        XCTAssertEqual(engine.run(for: "s1")?.phase, .done, "a fully-successful ×2 chain ends done")
        XCTAssertEqual(engine.run(for: "s1")?.repetition, 1, "the run settles on repetition index 1")
    }

    func testChainOnFailStop() async {
        let store = freshStore()
        seedNewestFirst(store, "s1", ["c", "b", "a"])
        store.updateStackConfig("s1") { $0.loopCount = 3 }
        let mock = Mock(store: store, outcomes: ["a": .completed, "b": .failed])
        let engine = StackRunEngine(seams: mock.seams())
        engine.runStack("s1", .run, defaults)
        await settle()
        XCTAssertEqual(refs(mock), ["a", "b"], "on-fail 'stop' halts immediately")
        XCTAssertEqual(engine.run(for: "s1")?.phase, .error, "a halted chain ends error")
    }

    func testChainOnFailContinue() async {
        let store = freshStore()
        seedNewestFirst(store, "s1", ["c", "b", "a"])
        store.updateStackConfig("s1") { $0.loopCount = 2; $0.guardrails = StackGuardrails(onFail: .continue, budget: .auto) }
        let mock = Mock(store: store, outcomes: ["a": .completed, "b": .failed, "c": .completed])
        let engine = StackRunEngine(seams: mock.seams())
        engine.runStack("s1", .run, defaults)
        await settle()
        XCTAssertEqual(refs(mock), ["a", "b", "c", "a", "b", "c"], "'continue' presses past the failed card and repeats")
        XCTAssertEqual(engine.run(for: "s1")?.phase, .error, "still error overall — a failure happened")
    }

    func testChainOnFailBackoff() async {
        let store = freshStore()
        seedNewestFirst(store, "s1", ["c", "b", "a"])
        store.updateStackConfig("s1") { $0.loopCount = 2; $0.guardrails = StackGuardrails(onFail: .backoff, budget: .auto) }
        let mock = Mock(store: store, outcomes: ["a": .completed, "b": .failed, "c": .completed])
        let engine = StackRunEngine(seams: mock.seams())
        engine.runStack("s1", .run, defaults)
        await settle()
        XCTAssertEqual(refs(mock), ["a", "b", "a", "b"], "'backoff' skips the rest of a failed pass, still tries next rep")
        XCTAssertEqual(engine.run(for: "s1")?.phase, .error, "a chain that never completed a clean pass reports error")
    }

    func testRunUntilGoalMet() async {
        let store = freshStore()
        seedNewestFirst(store, "s1", ["b", "a"])
        store.updateStackConfig("s1") {
            $0.loopCount = 0
            $0.evals = [BASELINE_EVAL, EvalRef(name: "tests pass", tier: .test)]
            $0.goal = StackGoal(pursue: true, noProgressLimit: 3)
        }
        let mock = Mock(store: store, outcomes: [
            "a": .completed, "b": .completed,
            "s1::stack-eval::0": .failed, "s1::stack-eval::1": .completed
        ])
        let engine = StackRunEngine(seams: mock.seams())
        engine.runStack("s1", .run, defaults)
        await settle()
        XCTAssertEqual(refs(mock), ["a", "b", "s1::stack-eval::0", "a", "b", "s1::stack-eval::1"],
                       "the chain re-runs, evaluating acceptance after each pass, until the goal passes")
        XCTAssertEqual(engine.run(for: "s1")?.phase, .done, "meeting the goal ends done")
        XCTAssertEqual(engine.run(for: "s1")?.stopReason, .goalMet, "the recorded reason is goal_met")
    }

    func testStackEvalTaskCarriesAcceptance() async {
        let store = freshStore()
        seedNewestFirst(store, "s1", ["a"])
        store.updateStackConfig("s1") {
            $0.loopCount = 1
            $0.evals = [BASELINE_EVAL, EvalRef(name: "tests pass", tier: .test)]
            $0.goal = StackGoal(pursue: true, noProgressLimit: 3)
        }
        let mock = Mock(store: store, outcomes: ["a": .completed, "s1::stack-eval::0": .completed])
        let engine = StackRunEngine(seams: mock.seams())
        engine.runStack("s1", .run, defaults)
        await settle()
        let evalPost = mock.captured.first { $0.clientRef == "s1::stack-eval::0" }
        XCTAssertNotNil(evalPost, "a dedicated stack-acceptance eval task is launched")
        XCTAssertNotNil(evalPost?.payload.options.acceptance, "the eval task carries the compiled acceptance")
        XCTAssertEqual(evalPost?.payload.options.maxIterations, 1, "the stack eval is a single attempt")
        XCTAssertEqual(engine.run(for: "s1")?.stopReason, .goalMet, "a passing single-run stack meets its goal")
    }

    func testGoalHaltsOnChainCeiling() async {
        let store = freshStore()
        seedNewestFirst(store, "s1", ["a"])
        store.updateStackConfig("s1") {
            $0.loopCount = 2
            $0.evals = [BASELINE_EVAL, EvalRef(name: "tests pass", tier: .test)]
            $0.goal = StackGoal(pursue: true, noProgressLimit: 5)
        }
        let mock = Mock(store: store, outcomes: ["a": .completed, "s1::stack-eval::0": .failed, "s1::stack-eval::1": .failed])
        let engine = StackRunEngine(seams: mock.seams())
        engine.runStack("s1", .run, defaults)
        await settle()
        XCTAssertEqual(refs(mock), ["a", "s1::stack-eval::0", "a", "s1::stack-eval::1"], "re-runs up to the loopCount ceiling")
        XCTAssertEqual(engine.run(for: "s1")?.phase, .error, "giving up without the goal ends error")
        XCTAssertEqual(engine.run(for: "s1")?.stopReason, .maxChainLoops, "the reason is the specific max_chain_loops")
    }

    func testGoalHaltsOnNoProgress() async {
        let store = freshStore()
        seedNewestFirst(store, "s1", ["a"])
        store.updateStackConfig("s1") {
            $0.loopCount = 0
            $0.evals = [BASELINE_EVAL, EvalRef(name: "tests pass", tier: .test)]
            $0.goal = StackGoal(pursue: true, noProgressLimit: 2)
        }
        let mock = Mock(store: store,
                        outcomes: ["a": .completed, "s1::stack-eval::0": .failed, "s1::stack-eval::1": .failed, "s1::stack-eval::2": .failed],
                        scores: ["s1::stack-eval::0": 0.5, "s1::stack-eval::1": 0.5, "s1::stack-eval::2": 0.5])
        let engine = StackRunEngine(seams: mock.seams())
        engine.runStack("s1", .run, defaults)
        await settle(800)
        XCTAssertEqual(engine.run(for: "s1")?.stopReason, .noProgress, "a stalled eval score halts with no_progress")
        XCTAssertEqual(engine.run(for: "s1")?.phase, .error, "a no-progress halt is not a success")
    }

    func testRunOnceNeverPursues() async {
        let store = freshStore()
        seedNewestFirst(store, "s1", ["a"])
        store.updateStackConfig("s1") {
            $0.loopCount = 1
            $0.evals = [BASELINE_EVAL, EvalRef(name: "tests pass", tier: .test)]
            $0.goal = StackGoal(pursue: true, noProgressLimit: 3)
        }
        let mock = Mock(store: store, outcomes: ["a": .completed])
        let engine = StackRunEngine(seams: mock.seams())
        engine.runStack("s1", .runOnce, defaults)
        await settle()
        XCTAssertEqual(refs(mock), ["a"], "Run once runs one pass, launches no stack eval, even with a goal set")
        XCTAssertEqual(engine.run(for: "s1")?.phase, .done, "clean run-once ends done, no pursuit")
        XCTAssertNil(engine.run(for: "s1")?.stopReason, "no stop reason when no goal was pursued")
    }

    func testPursueBaselineOnlyIsInert() async {
        let store = freshStore()
        seedNewestFirst(store, "s1", ["a"])
        store.updateStackConfig("s1") { $0.loopCount = 1; $0.goal = StackGoal(pursue: true, noProgressLimit: 3) }
        let mock = Mock(store: store, outcomes: ["a": .completed])
        let engine = StackRunEngine(seams: mock.seams())
        engine.runStack("s1", .run, defaults)
        await settle()
        XCTAssertEqual(refs(mock), ["a"], "pursue with baseline-only acceptance launches no eval (legacy fallback)")
        XCTAssertNil(engine.run(for: "s1")?.stopReason, "no goal pursued → no stop reason")
    }

    func testBarePaneLaunch() async {
        let store = freshStore()
        seedNewestFirst(store, "s1", ["c1"])
        // Use a card whose goal is explicit for the assertions.
        store.updateCardInPane("s1", store.pane(for: "s1")!.cards[0].id) { $0.goal = "summarize main.rs" }
        let mock = Mock(store: store, outcomes: [:])
        mock.bareTaskId = "bare-task-1"
        mock.outcomes["bare-task-1"] = .completed
        let engine = StackRunEngine(seams: mock.seams())
        engine.runBarePane("s1", defaults)
        await settle()
        XCTAssertEqual(mock.captured.count, 1, "bare pane launches exactly one task")
        let b = mock.captured[0].payload
        XCTAssertEqual(b.goal, "summarize main.rs", "bare payload carries the card goal")
        XCTAssertEqual(b.repo, "r", "bare payload falls back to the pane default repo")
        XCTAssertNil(b.options.maxIterations, "bare payload omits stack-loop semantics")
        XCTAssertNil(b.options.onFail)
        XCTAssertNil(b.options.clientRef)
        XCTAssertEqual(engine.run(for: "s1")?.phase, .done, "bare run reaches done")
        let wired = store.pane(for: "s1")!.cards[0]
        XCTAssertEqual(wired.status, .done, "the card is marked done")
        XCTAssertEqual(wired.taskId, "bare-task-1", "the card carries the launched task id")
    }

    func testBarePaneNoOpForZeroOrManyCards() async {
        let store = freshStore()
        let mock = Mock(store: store, outcomes: [:])
        let engine = StackRunEngine(seams: mock.seams())
        engine.runBarePane("s1", defaults) // 0 cards
        await settle(50)
        seedNewestFirst(store, "s2", ["y", "x"])
        engine.runBarePane("s2", defaults) // 2 cards
        await settle(50)
        XCTAssertEqual(mock.captured.count, 0, "runBarePane is a no-op for 0-card and 2+-card panes")
    }
}
