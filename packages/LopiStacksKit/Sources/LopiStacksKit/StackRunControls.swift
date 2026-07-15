import Foundation

// Run-control surface for `StackRunEngine` — bare-pane launch, pause/resume/
// drain, bump, and schedule-stack. Split out of `StackRun.swift` to keep each
// file under the 500-line rule. Foundation only.

public extension StackRunEngine {
    // MARK: Bare-pane launch (F2)

    /// Launch a *bare* pane's single staged card. A ≤1-card pane never renders
    /// the dock, so this is its run affordance: submits the one card through the
    /// loop-semantics-free `paneSubmitPayload` and wires taskId + terminal status
    /// back onto the card, exactly as `advance`'s single-card section does but
    /// with no chain/repetition/goal machinery. No-op unless the pane has exactly
    /// one card and isn't already running.
    func runBarePane(_ paneKey: String, _ defaults: PaneDefaults) {
        guard let pane = seams.panes().first(where: { $0.key == paneKey }), pane.cards.count == 1 else { return }
        if run(for: paneKey)?.phase == .running { return }
        let card = pane.cards[0]
        putRun(StackRunState(
            paneKey: paneKey, phase: .running, intent: .runOnce, order: [card.id],
            cursor: 0, repetition: 0, loopTarget: 1, onFail: .stop, hadFailure: false,
            error: nil, acceptance: nil, noProgressLimit: 0, noGainStreak: 0,
            goalBest: nil, stopReason: nil))
        Task { await launchBareCard(paneKey, card, defaults) }
    }

    private func launchBareCard(_ paneKey: String, _ card: StackCard, _ defaults: PaneDefaults) async {
        let payload = paneSubmitPayload(PaneLaunch(
            goal: card.goal,
            repo: card.config.repo ?? defaults.repo,
            priority: "normal",
            model: card.config.model ?? defaults.model,
            effort: card.config.effort ?? defaults.effort,
            branch: card.config.branch))
        seams.updateCard(paneKey, card.id) { $0.status = .queued }
        let taskId: String
        do {
            taskId = try await seams.createTask(payload)
        } catch {
            seams.updateCard(paneKey, card.id) { $0.status = .idle }
            setRun(paneKey) {
                $0.phase = .error; $0.hadFailure = true
                $0.error = "\"\(card.goal)\" failed to launch: \(error.localizedDescription)"
            }
            return
        }
        seams.updateCard(paneKey, card.id) { $0.status = .running; $0.taskId = taskId }
        let terminal = await seams.waitForTerminal(taskId)
        seams.updateCard(paneKey, card.id) { $0.status = .done }
        setRun(paneKey) {
            if terminal == .completed {
                $0.phase = .done; $0.error = nil
            } else {
                $0.phase = .error; $0.hadFailure = true; $0.error = "\"\(card.goal)\" ended \(terminal.rawValue)"
            }
        }
    }

    // MARK: Pause / resume / drain

    /// Halt after the currently-running card reaches a terminal status;
    /// resumable via `resumeStack`. No-op if there's no active run.
    func pauseStack(_ paneKey: String) {
        guard run(for: paneKey)?.phase == .running else { return }
        setRun(paneKey) { $0.phase = .paused }
    }

    /// Continue a paused run from where it left off. No-op unless paused.
    func resumeStack(_ paneKey: String, _ defaults: PaneDefaults) {
        guard run(for: paneKey)?.phase == .paused else { return }
        setRun(paneKey) { $0.phase = .running }
        Task { await advance(paneKey, defaults) }
    }

    /// Let the current card finish, then stop for good (not resumable).
    func drainStack(_ paneKey: String) {
        guard let state = run(for: paneKey) else { return }
        if state.phase == .paused {
            setRun(paneKey) {
                $0.phase = state.hadFailure ? .error : .done
                $0.error = state.hadFailure ? (state.error ?? "drained after at least one failed loop") : nil
            }
        } else if state.phase == .running {
            setRun(paneKey) { $0.phase = .draining }
        }
    }

    // MARK: Bump a queued card within an active run

    /// Reorder a not-yet-started card within an active run's remaining queue,
    /// reflecting the swap into both the run's plan (`order`) and the pane's card
    /// array. Rejects illegal transitions with a clear error.
    func bumpCard(_ paneKey: String, _ cardId: String, _ direction: BumpDirection) -> BumpResult {
        guard let state = run(for: paneKey) else { return .err("no active run for this pane") }
        let idx = state.order.firstIndex(of: cardId)
        let neighborId: String? = idx.flatMap { i in
            let n = direction == .up ? i - 1 : i + 1
            return state.order.indices.contains(n) ? state.order[n] : nil
        }
        let result = bumpInOrder(state.order, state.cursor, cardId, direction)
        guard case let .ok(nextOrder) = result else { return result }
        setRun(paneKey) { $0.order = nextOrder }
        if let neighborId, let pane = seams.panes().first(where: { $0.key == paneKey }) {
            let fromIdx = pane.cards.firstIndex(where: { $0.id == cardId })
            let neighborIdx = pane.cards.firstIndex(where: { $0.id == neighborId })
            if let fromIdx, let neighborIdx {
                seams.reorderPaneCards(paneKey, fromIdx, neighborIdx)
            }
        }
        return .ok(nextOrder)
    }

    // MARK: Schedule stack (honest: only the bottom card)

    /// Run-menu "Schedule stack": attaches one cron to the first card in
    /// execution order via the real schedule endpoint; every other card is
    /// reported back as skipped rather than pretended-scheduled.
    func scheduleStack(_ paneKey: String, _ cronExpr: String, _ defaults: PaneDefaults) async -> ScheduleStackResult {
        guard let pane = seams.panes().first(where: { $0.key == paneKey }), !pane.cards.isEmpty else {
            return ScheduleStackResult(ok: false, scheduledCardId: nil, skippedCardIds: [], error: "nothing to schedule")
        }
        let ordered = executionOrder(pane.cards)
        let first = ordered[0]
        let rest = Array(ordered.dropFirst())
        let payload = cardToTaskPayload(first, defaults)
        do {
            try await seams.createSchedule("stack:\(paneKey):\(first.id)", cronExpr, payload.goal, payload.repo, payload.priority)
        } catch {
            return ScheduleStackResult(ok: false, scheduledCardId: nil, skippedCardIds: rest.map(\.id), error: error.localizedDescription)
        }
        return ScheduleStackResult(ok: true, scheduledCardId: first.id, skippedCardIds: rest.map(\.id), error: nil)
    }

    /// Clear a pane's run state (dismiss its banner / delete-with-run).
    func clearRun(_ paneKey: String) { removeRun(paneKey) }
}
