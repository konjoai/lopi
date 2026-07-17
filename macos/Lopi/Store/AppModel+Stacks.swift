import Foundation
import LopiStacksKit

// Wires the pure `StackRunEngine` to the real macOS backend paths — the app-side
// seam layer. This is the ONLY place the framework-free sequencer touches
// `LopiClient`/`liveAgents`, exactly the boundary the engine's injected
// `StackRunSeams` were designed for (mirroring how the web module takes
// `statusSource` as a parameter). Reuses the same `createTask`/event-stream
// paths Fix-2/Verify-2 already proved — no new plumbing invented here.

/// The minimal decode of a `POST /api/tasks` response — enough to recover the
/// effective task id (`duplicate_of ?? id`, mirroring web's `effectiveTaskId`),
/// so a card traces back to its launched task even under server-side dedup.
private struct CreatedTaskResponse: Decodable {
    let id: String
    let duplicateOf: String?
    enum CodingKeys: String, CodingKey {
        case id
        case duplicateOf = "duplicate_of"
    }
    var effectiveId: String { duplicateOf ?? id }
}

extension AppModel {
    /// Build the sequencer's injected seams over this model's real backend.
    func makeStackSeams() -> StackRunSeams {
        StackRunSeams(
            panes: { [stackStore] in stackStore.panes },
            updateCard: { [stackStore] key, id, mutate in stackStore.updateCardInPane(key, id, mutate) },
            createTask: { [weak self] payload in
                guard let self else { throw LopiError.transport("model deallocated") }
                return try await self.launchStackTask(payload)
            },
            waitForTerminal: { [weak self] taskId in
                await self?.awaitTerminal(taskId) ?? .cancelled
            },
            score: { [weak self] taskId in self?.liveAgents[taskId]?.testPassRate },
            createScheduleChain: { [weak self] name, cron, goals, repo, priority, onFail in
                guard let self else { throw LopiError.transport("model deallocated") }
                let chain = try await self.client.createScheduleChain(ScheduleChainBody(
                    name: name, cron: cron,
                    steps: goals.map { ScheduleChainStepBody(goal: $0, allowedDirs: nil, forbiddenDirs: nil) },
                    repo: repo.isEmpty ? nil : repo, priority: priority,
                    autonomyLevel: nil, onFail: onFail.rawValue, enabled: true))
                return chain.id
            },
            reorderPaneCards: { [stackStore] key, from, to in stackStore.reorderInPane(key, from, to) })
    }

    /// The stack control dock's "schedule the entire stack" toggle/cron-edit:
    /// creates, updates, enables, or disables the pane's `/api/schedule-chains`
    /// row to match `config.scheduled`/`config.cron`, and persists the returned
    /// chain id on the pane (`StackConfig.chainId`) so the next edit updates the
    /// same row instead of creating a duplicate. Mirrors
    /// `web/src/lib/stores/stackRun.ts::syncStackSchedule`. Best-effort — a
    /// failed sync leaves the client's toggle state as the source of truth
    /// until the next edit retries.
    func syncStackSchedule(paneKey: String, defaults: PaneDefaults) {
        guard let pane = stackStore.panes.first(where: { $0.key == paneKey }) else { return }
        let config = pane.config
        Task { [weak self] in
            guard let self else { return }
            guard config.scheduled else {
                if let chainId = config.chainId {
                    try? await self.client.setScheduleChainEnabled(id: chainId, enabled: false)
                }
                return
            }
            let ordered = executionOrder(pane.cards)
            guard !ordered.isEmpty else { return }
            let first = cardToTaskPayload(ordered[0], defaults)
            let body = ScheduleChainBody(
                name: "stack:\(paneKey)", cron: buildCronString(config.cron),
                steps: ordered.map { ScheduleChainStepBody(goal: cardToTaskPayload($0, defaults).goal, allowedDirs: nil, forbiddenDirs: nil) },
                repo: first.repo, priority: first.priority,
                autonomyLevel: nil, onFail: config.guardrails.onFail.rawValue, enabled: true)
            do {
                let chain: ScheduleChain
                if let chainId = config.chainId {
                    chain = try await self.client.updateScheduleChain(id: chainId, body)
                    try await self.client.setScheduleChainEnabled(id: chainId, enabled: true)
                } else {
                    chain = try await self.client.createScheduleChain(body)
                }
                self.stackStore.updateStackConfig(paneKey) { $0.chainId = chain.id }
            } catch {
                // Best-effort — see doc comment above.
            }
        }
    }

    /// Submit one card/pane payload through the real create-task path and return
    /// the effective task id. Maps the pure `StackTaskPayload` onto the backend
    /// `CreateTaskBody` — carrying the WIRED loop-limit fields (max_iterations /
    /// on_fail / gate / until / client_ref) now that the body models them.
    private func launchStackTask(_ payload: StackTaskPayload) async throws -> String {
        let o = payload.options
        let body = CreateTaskBody(
            goal: payload.goal,
            repo: payload.repo.isEmpty ? nil : payload.repo,
            priority: payload.priority,
            constraints: o.constraints,
            allowedDirs: nil, forbiddenDirs: nil, maxRetries: nil,
            model: o.model, effort: o.effort,
            maxIterations: o.maxIterations, onFail: o.onFail?.rawValue,
            gate: o.gate, until: o.until, clientRef: o.clientRef)
        let data = try await client.createTask(body)
        await refreshTasks()
        let resp = try JSONDecoder().decode(CreatedTaskResponse.self, from: data)
        return resp.effectiveId
    }

    /// Resolve once the task reaches a terminal fleet bucket, reusing the live
    /// event-stream–fed `liveAgents` map (no new polling transport — the WS
    /// stream already keeps it live). Polls the observable map every 300 ms; a
    /// generous cap keeps a task that never appears from suspending forever.
    private func awaitTerminal(_ taskId: String) async -> TerminalStatus {
        let maxPolls = 2 * 60 * 60 * 1000 / 300 // ~2h ceiling
        for _ in 0..<maxPolls {
            if let agent = liveAgents[taskId] {
                switch FleetBucket.of(agent.phase) {
                case .succeeded: return .completed
                case .failed: return .failed
                case .cancelled: return .cancelled
                case .running, .queued: break
                }
            }
            try? await Task.sleep(nanoseconds: 300_000_000)
        }
        return .failed
    }
}
