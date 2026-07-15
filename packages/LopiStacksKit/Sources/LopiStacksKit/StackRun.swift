import Foundation
import Observation

// Client-side stack-run sequencer — the functional port of `stores/stackRun.ts`.
// There is no server-side "stack": running a stack is a small state machine that
// submits one card's task at a time via the real create-task path, waits for it
// to reach a terminal status, and only checks pause/drain *between* cards. That
// gives "pause = halt after the current iteration" and "drain = finish then
// stop" without interrupting a running task's internal retry loop.
//
// Framework surface is limited to Foundation + Observation (the analogue of the
// web module's svelte `writable`) — ZERO SwiftUI/AppKit, so it lifts into a
// shared package unchanged. All side-effecting seams (create-task, terminal-
// status wait, live score, card-status writes, schedule) are injected via
// `StackRunSeams`, exactly the reason the web module takes `statusSource` as a
// parameter instead of importing `./agents` — which is what lets the unit tests
// substitute a deterministic mock backend.

/// Which run-menu action started this run.
public enum RunIntent: Equatable { case run, runOnce }

/// A run's lifecycle. `paused` is resumable; `draining` finalizes to `done`
/// once the in-flight card's wait resolves, and does not resume.
public enum RunPhase: String, Equatable {
    case idle, running, paused, draining, done, error
}

/// A card task's terminal outcome.
public enum TerminalStatus: String, Equatable { case completed, failed, cancelled }

/// One pane's active (or just-finished) run. `order`/`cursor` are a snapshot of
/// execution order taken at launch time; `loopTarget`/`onFail` are likewise
/// snapshotted from `pane.config` — tweaking the dock mid-run never reshuffles a
/// run already in flight. `acceptance`/`noProgressLimit`/`noGainStreak`/
/// `goalBest`/`stopReason` are the B1 run-until-goal state.
public struct StackRunState: Equatable {
    public init(
        paneKey: String,
        phase: RunPhase,
        intent: RunIntent,
        order: [String],
        cursor: Int,
        repetition: Int,
        loopTarget: Int,
        onFail: OnFail,
        hadFailure: Bool,
        error: String?,
        acceptance: StackAcceptance?,
        noProgressLimit: Int,
        noGainStreak: Int,
        goalBest: Double?,
        stopReason: StackStopReason?
    ) {
        self.paneKey = paneKey
        self.phase = phase
        self.intent = intent
        self.order = order
        self.cursor = cursor
        self.repetition = repetition
        self.loopTarget = loopTarget
        self.onFail = onFail
        self.hadFailure = hadFailure
        self.error = error
        self.acceptance = acceptance
        self.noProgressLimit = noProgressLimit
        self.noGainStreak = noGainStreak
        self.goalBest = goalBest
        self.stopReason = stopReason
    }

    public var paneKey: String
    public var phase: RunPhase
    public var intent: RunIntent
    public var order: [String]
    public var cursor: Int
    public var repetition: Int
    public var loopTarget: Int
    public var onFail: OnFail
    public var hadFailure: Bool
    public var error: String?
    public var acceptance: StackAcceptance?
    public var noProgressLimit: Int
    public var noGainStreak: Int
    public var goalBest: Double?
    public var stopReason: StackStopReason?
}

/// The injected side-effecting seams — the only non-pure surface the engine
/// touches. In production these are wired to `AppModel`/`LopiClient`; in tests
/// they're a deterministic mock (mirroring the web `mockBackend`).
public struct StackRunSeams {
    public init(
        panes: @escaping () -> [StackPaneState],
        updateCard: @escaping (_ paneKey: String, _ cardId: String, _ mutate: (inout StackCard) -> Void) -> Void,
        createTask: @escaping (_ payload: StackTaskPayload) async throws -> String,
        waitForTerminal: @escaping (_ taskId: String) async -> TerminalStatus,
        score: @escaping (_ taskId: String) -> Double?,
        createSchedule: @escaping (_ name: String, _ cron: String, _ goal: String, _ repo: String, _ priority: String) async throws -> Void,
        reorderPaneCards: @escaping (_ paneKey: String, _ from: Int, _ to: Int) -> Void
    ) {
        self.panes = panes
        self.updateCard = updateCard
        self.createTask = createTask
        self.waitForTerminal = waitForTerminal
        self.score = score
        self.createSchedule = createSchedule
        self.reorderPaneCards = reorderPaneCards
    }

    /// A fresh snapshot of the pane list.
    public var panes: () -> [StackPaneState]
    /// Patch one card in a pane (status/taskId writes as the run progresses).
    public var updateCard: (_ paneKey: String, _ cardId: String, _ mutate: (inout StackCard) -> Void) -> Void
    /// Submit a task; returns the effective task id (the response's
    /// `duplicate_of ?? id`, so a client_ref traces back even under dedup).
    public var createTask: (_ payload: StackTaskPayload) async throws -> String
    /// Resolve once `taskId` reaches a terminal status.
    public var waitForTerminal: (_ taskId: String) async -> TerminalStatus
    /// The live scalar score for a finished task, if any (drives no-progress).
    public var score: (_ taskId: String) -> Double?
    /// Attach one cron via the real schedule endpoint.
    public var createSchedule: (_ name: String, _ cron: String, _ goal: String, _ repo: String, _ priority: String) async throws -> Void
    /// Reorder a pane's card array (so the rendered order matches a mid-run bump).
    public var reorderPaneCards: (_ paneKey: String, _ from: Int, _ to: Int) -> Void
}

/// The result of `scheduleStack` — honest that it can only attach the cron to
/// the bottom-of-stack (first-to-run) card, not the whole plan.
public struct ScheduleStackResult: Equatable {
    public init(
        ok: Bool,
        scheduledCardId: String?,
        skippedCardIds: [String],
        error: String?
    ) {
        self.ok = ok
        self.scheduledCardId = scheduledCardId
        self.skippedCardIds = skippedCardIds
        self.error = error
    }

    public var ok: Bool
    public var scheduledCardId: String?
    public var skippedCardIds: [String]
    public var error: String?
}

/// The stack-run sequencer. Owns the per-pane `runs` map; drives launches via
/// the injected seams. `@Observable` so SwiftUI re-renders on run-state change
/// (the svelte `writable` analogue).
@Observable
@MainActor
public final class StackRunEngine {
    /// Active runs, keyed by pane key. In-memory — a relaunch loses run state.
    public private(set) var runs: [String: StackRunState] = [:]

    @ObservationIgnored public let seams: StackRunSeams

    public init(seams: StackRunSeams) {
        self.seams = seams
    }

    public func run(for paneKey: String) -> StackRunState? { runs[paneKey] }

    /// Install a fresh run state (bare-pane launch).
    public func putRun(_ state: StackRunState) { runs[state.paneKey] = state }

    /// Drop a pane's run state.
    public func removeRun(_ paneKey: String) { runs.removeValue(forKey: paneKey) }

    /// Patch an existing run in place. Internal so the controls extension shares
    /// the one mutation path.
    public func setRun(_ paneKey: String, _ patch: (inout StackRunState) -> Void) {
        guard var current = runs[paneKey] else { return }
        patch(&current)
        runs[paneKey] = current
    }

    private func findCard(_ paneKey: String, _ cardId: String) -> StackCard? {
        seams.panes().first { $0.key == paneKey }?.cards.first { $0.id == cardId }
    }

    // MARK: Run-menu "Run now" / "Run once"

    /// Launch a fresh run for this pane's cards in execution order. B1 — under a
    /// plain "Run" with a real goal (`stackPursuesGoal`), the compiled acceptance
    /// is snapshotted too, flipping the chain from fixed-count to run-until-goal.
    /// "Run once" never pursues.
    public func runStack(_ paneKey: String, _ intent: RunIntent, _ defaults: PaneDefaults) {
        guard let pane = seams.panes().first(where: { $0.key == paneKey }), !pane.cards.isEmpty else { return }
        let order = executionOrder(pane.cards).map(\.id)
        let pursuing = intent == .run && stackPursuesGoal(pane.config)
        let acceptance = pursuing ? evalsToAcceptance(pane.config.evals) : nil
        runs[paneKey] = StackRunState(
            paneKey: paneKey, phase: .running, intent: intent, order: order,
            cursor: 0, repetition: 0, loopTarget: pane.config.loopCount,
            onFail: pane.config.guardrails.onFail, hadFailure: false, error: nil,
            acceptance: acceptance, noProgressLimit: pane.config.goal.noProgressLimit,
            noGainStreak: 0, goalBest: nil, stopReason: nil)
        Task { await advance(paneKey, defaults) }
    }

    // MARK: The driver

    /// Launch queued cards one at a time until the run pauses, drains, errors, or
    /// runs out of cards — then either starts the next repetition or (B1)
    /// evaluates the goal and re-runs, or finishes. Re-reads `runs`/`panes` fresh
    /// at the top of each pass, which is what makes an infinite (`loopTarget ==
    /// 0`) chain safe: every pass re-checks pause/drain first.
    public func advance(_ paneKey: String, _ defaults: PaneDefaults) async {
        while true {
            guard let state = runs[paneKey] else { return }
            switch state.phase {
            case .paused: return
            case .draining:
                finishDraining(paneKey, state); return
            case .running: break
            default: return
            }

            if state.cursor >= state.order.count {
                if state.acceptance != nil {
                    if await pursueGoal(paneKey, state, defaults) == .stop { return }
                    continue
                }
                if advanceChainBoundary(paneKey, state) == .stop { return }
                continue
            }

            if await launchNextCard(paneKey, state, defaults) == .stop { return }
        }
    }

    private enum Step: Equatable { case cont, stop }

    private func finishDraining(_ paneKey: String, _ state: StackRunState) {
        setRun(paneKey) {
            $0.phase = state.hadFailure ? .error : .done
            $0.error = state.hadFailure ? (state.error ?? "drained after at least one failed loop") : nil
        }
    }

    /// A full pass over the chain just completed with no goal: fixed-`loopTarget`
    /// repetition, unchanged from legacy behavior.
    private func advanceChainBoundary(_ paneKey: String, _ state: StackRunState) -> Step {
        let nextRepetition = state.repetition + 1
        let moreRepetitions = state.loopTarget == 0 || nextRepetition < state.loopTarget
        if moreRepetitions {
            setRun(paneKey) { $0.cursor = 0; $0.repetition = nextRepetition }
            return .cont
        }
        setRun(paneKey) {
            $0.phase = state.hadFailure ? .error : .done
            $0.error = state.hadFailure ? (state.error ?? "chain completed with at least one failed loop") : nil
        }
        return .stop
    }

    /// Launch the card at the cursor, wire status back, apply the chain on-fail
    /// policy on a non-completed outcome. Mirrors `advance`'s single-card section.
    private func launchNextCard(_ paneKey: String, _ state: StackRunState, _ defaults: PaneDefaults) async -> Step {
        let cardId = state.order[state.cursor]
        guard let card = findCard(paneKey, cardId) else {
            setRun(paneKey) { $0.cursor = state.cursor + 1 } // removed mid-run — skip
            return .cont
        }
        let payload = state.intent == .runOnce
            ? cardToTaskPayloadForRunOnce(card, defaults)
            : cardToTaskPayload(card, defaults)

        seams.updateCard(paneKey, cardId) { $0.status = .queued }
        let taskId: String
        do {
            taskId = try await seams.createTask(payload)
        } catch {
            seams.updateCard(paneKey, cardId) { $0.status = .idle }
            setRun(paneKey) {
                $0.phase = .error; $0.hadFailure = true
                $0.error = "\"\(card.goal)\" failed to launch: \(error.localizedDescription)"
            }
            return .stop
        }
        seams.updateCard(paneKey, cardId) { $0.status = .running; $0.taskId = taskId }
        let terminal = await seams.waitForTerminal(taskId)
        seams.updateCard(paneKey, cardId) { $0.status = .done }
        return applyCardOutcome(paneKey, state, card, terminal)
    }

    /// Fold one card's terminal outcome into the run per the chain on-fail policy.
    private func applyCardOutcome(_ paneKey: String, _ state: StackRunState, _ card: StackCard, _ terminal: TerminalStatus) -> Step {
        if terminal == .completed {
            setRun(paneKey) { $0.cursor = state.cursor + 1 }
            return .cont
        }
        let error = "\"\(card.goal)\" ended \(terminal.rawValue)"
        switch state.onFail {
        case .stop:
            setRun(paneKey) { $0.phase = .error; $0.hadFailure = true; $0.error = error }
            return .stop
        case .continue:
            setRun(paneKey) { $0.cursor = state.cursor + 1; $0.hadFailure = true; $0.error = error }
            return .cont
        case .backoff:
            setRun(paneKey) { $0.cursor = state.order.count; $0.hadFailure = true; $0.error = error }
            return .cont
        }
    }

    // MARK: B1 run-until-goal

    /// One run-until-goal step after a chain-run completes. Evaluates the stack
    /// acceptance and either stops `goal_met`, stops with a specific reason, or
    /// re-runs the whole chain. Reuses A3's precedence + gain idea at chain scope.
    private func pursueGoal(_ paneKey: String, _ state: StackRunState, _ defaults: PaneDefaults) async -> Step {
        guard let verdict = await evaluateStackAcceptance(paneKey, state, defaults) else { return .stop }
        let chainRun = state.repetition + 1
        if verdict.passed {
            setRun(paneKey) { $0.phase = .done; $0.stopReason = .goalMet; $0.error = nil }
            return .stop
        }
        let gain = foldGain(GainState(best: state.goalBest, streak: state.noGainStreak), verdict.score)
        let decision = decideAfterMiss(GoalPursuit(
            chainRun: chainRun, maxChainLoops: state.loopTarget,
            noGainStreak: gain.streak, noProgressLimit: state.noProgressLimit))
        switch decision {
        case .rerun:
            setRun(paneKey) {
                $0.cursor = 0; $0.repetition = chainRun
                $0.goalBest = gain.best; $0.noGainStreak = gain.streak
            }
            return .cont
        case let .stop(reason):
            setRun(paneKey) {
                $0.phase = .error; $0.stopReason = reason
                $0.goalBest = gain.best; $0.noGainStreak = gain.streak
                $0.error = stackStopLabel(reason)
            }
            return .stop
        }
    }

    /// The stack-scope eval seam: a dedicated task carrying the compiled
    /// acceptance; its terminal status *is* the stack-level verdict (A1 makes a
    /// task complete iff its acceptance passed), and the live score surfaces the
    /// no-progress scalar. Single attempt. Returns `nil` (and errors the run)
    /// only if the eval task can't even launch.
    private func evaluateStackAcceptance(_ paneKey: String, _ state: StackRunState, _ defaults: PaneDefaults) async -> (passed: Bool, score: Double?)? {
        guard let acceptance = state.acceptance else { return (true, nil) }
        let evalRef = "\(paneKey)::stack-eval::\(state.repetition)"
        var options = StackTaskOptions()
        options.model = defaults.model
        options.effort = defaults.effort
        options.maxIterations = 1
        options.acceptance = acceptance
        options.clientRef = evalRef
        let payload = StackTaskPayload(goal: stackGoalPrompt(paneKey), repo: defaults.repo, priority: "normal", options: options)
        let taskId: String
        do {
            taskId = try await seams.createTask(payload)
        } catch {
            setRun(paneKey) {
                $0.phase = .error; $0.hadFailure = true
                $0.error = "stack acceptance eval failed to launch: \(error.localizedDescription)"
            }
            return nil
        }
        let terminal = await seams.waitForTerminal(taskId)
        return (terminal == .completed, seams.score(taskId))
    }

    private func stackGoalPrompt(_ paneKey: String) -> String {
        let title = seams.panes().first { $0.key == paneKey }?.title ?? paneKey
        return "verify stack acceptance for \"\(title)\""
    }
}
