import Foundation

// Translates the raw `/ws` event stream into the live cockpit state: per-agent
// cognition, the event ticker, and the cost/throughput time series.
extension AppModel {
    /// Maximum rows kept in the rolling ticker and the time-series buffers.
    private var feedCap: Int { 60 }
    private var seriesCap: Int { 90 }
    private var logCap: Int { 500 }

    /// Main entry point for a decoded live event.
    func ingest(_ event: AgentEvent) {
        switch event {
        case let .poolStats(s):
            stats.running = s.running
            stats.queued = s.queued
            stats.succeeded = s.succeeded
            stats.failed = s.failed
            stats.uptimeSecs = s.uptimeSecs

        case let .taskQueued(id, goal, priority):
            seedAgent(id: id, goal: goal, phase: "queued")
            push(.queued, "Task queued", priority.isEmpty ? goal : "\(priority) · \(goal)")
            scheduleTaskRefresh()

        case let .taskStarted(id, attempt, branch):
            mutateAgent(id) {
                $0.phase = "planning"
                $0.attempt = attempt
                $0.branch = branch.isEmpty ? nil : branch
                $0.active = true
                $0.stimulus = .now
                $0.stimulusKind = "request"
            }
            push(.started, "Agent started", branch.isEmpty ? "attempt \(attempt)" : branch)
            scheduleTaskRefresh()

        case let .statusChanged(id, status, attempt):
            mutateAgent(id) {
                $0.phase = status
                $0.attempt = attempt
                if PhaseStyle.isActive(status) {
                    $0.stimulus = .now
                    $0.stimulusKind = "request"
                }
            }
            push(.status, "Phase: \(status)", shortGoal(id))
            scheduleTaskRefresh()

        case let .logLine(id, line, level):
            recentLogs.append("[\(level)] \(line)")
            if recentLogs.count > logCap { recentLogs.removeFirst(recentLogs.count - logCap) }
            // Attach to the originating agent's strip (only if we know it, so a
            // stray log never spawns a phantom pane).
            if liveAgents[id] != nil {
                liveAgents[id]?.logTail.append(AgentLog(level: level, text: line))
                let tail = liveAgents[id]?.logTail.count ?? 0
                if tail > 12 { liveAgents[id]?.logTail.removeFirst(tail - 12) }
                if level == "error" {
                    liveAgents[id]?.stimulus = .now
                    liveAgents[id]?.stimulusKind = "failure"
                }
            }
            if level == "error" {
                push(.error, "Error", line)
            } else if level == "warn" {
                push(.warn, "Warning", line)
            }

        case let .scoreUpdated(id, pass, lint, diff):
            mutateAgent(id) {
                $0.testPassRate = pass
                $0.lintErrors = lint
                $0.diffLines = diff
                if pass >= 0.8 { $0.stimulus = .now; $0.stimulusKind = "success" }
            }
            push(.score, "Score \(Int(pass * 100))%", "\(lint) lint · \(diff) diff lines")

        case let .turnMetrics(id, pressure, activity, tps, cost):
            mutateAgent(id) {
                $0.pressure = pressure
                $0.activity = activity
                $0.tokensPerSec = tps
                $0.costUsd = cost
                if !PhaseStyle.isActive($0.phase) { $0.phase = "implementing" }
            }
            recomputeAggregates()

        case let .verifierVerdict(id, passed, gaps, confidence):
            mutateAgent(id) {
                $0.verdictPassed = passed
                $0.verdictConfidence = confidence
                $0.stimulus = .now
                $0.stimulusKind = passed ? "success" : "failure"
            }
            if passed {
                push(.verdictPass, "Verifier passed", String(format: "%.0f%% confidence", confidence * 100))
            } else {
                push(.verdictFail, "Verifier failed", gaps.first ?? "rubric not met")
            }

        case let .taskCompleted(id, outcome, attempts):
            let success = outcome.lowercased().contains("success")
            mutateAgent(id) {
                $0.phase = outcome
                $0.active = false
                $0.activity = 0
                $0.stimulus = .now
                $0.stimulusKind = success ? "success" : "failure"
            }
            let kind: FeedItem.Kind = success ? .completed : .error
            push(kind, "Completed: \(outcome)", "\(attempts) attempt\(attempts == 1 ? "" : "s")")
            recomputeAggregates()
            scheduleTaskRefresh()

        case let .taskCancelled(id):
            mutateAgent(id) { $0.phase = "cancelled"; $0.active = false; $0.activity = 0 }
            push(.cancelled, "Task cancelled", shortGoal(id))
            recomputeAggregates()
            scheduleTaskRefresh()

        case let .budgetExceeded(_, scope, limit, burned):
            lastBudget = BudgetBreach(scope: scope, limitUsd: limit, burnedUsd: burned, at: .now)
            push(.budget, "Budget exceeded (\(scope))", String(format: "$%.2f / $%.2f", burned, limit))

        case .other:
            break
        }
    }

    /// Insert a live agent if we haven't seen this task id yet.
    func seedAgent(id: String, goal: String, phase: String) {
        if liveAgents[id] == nil {
            liveAgents[id] = LiveAgent(id: id, goal: goal, phase: phase, attempt: 0)
        } else if !goal.isEmpty {
            liveAgents[id]?.goal = goal
        }
    }

    // MARK: Private helpers

    private func mutateAgent(_ id: String, _ change: (inout LiveAgent) -> Void) {
        var agent = liveAgents[id] ?? LiveAgent(id: id, goal: shortGoal(id), phase: "planning", attempt: 0)
        change(&agent)
        agent.lastUpdate = .now
        liveAgents[id] = agent
    }

    private func push(_ kind: FeedItem.Kind, _ title: String, _ detail: String) {
        feed.insert(FeedItem(kind: kind, title: title, detail: detail, at: .now), at: 0)
        if feed.count > feedCap { feed.removeLast(feed.count - feedCap) }
    }

    /// Recompute fleet-wide cost / throughput / activity and append samples.
    private func recomputeAggregates() {
        let agents = liveAgents.values
        let totalCost = agents.reduce(0) { $0 + $1.costUsd }
        let active = agents.filter { $0.active }
        let totalTps = active.reduce(0) { $0 + $1.tokensPerSec }
        let meanActivity = active.isEmpty ? 0 : active.reduce(0) { $0 + $1.activity } / Double(active.count)

        aggregateActivity = meanActivity
        appendSample(&costSeries, totalCost)
        appendSample(&throughputSeries, totalTps)
    }

    private func appendSample(_ series: inout [Double], _ value: Double) {
        series.append(value)
        if series.count > seriesCap { series.removeFirst(series.count - seriesCap) }
    }

    private func shortGoal(_ id: String) -> String {
        liveAgents[id]?.goal ?? tasks.first { $0.id == id }?.goal ?? String(id.prefix(8))
    }

    /// Coalesce task-list refreshes — many events can arrive per second.
    private func scheduleTaskRefresh() {
        Task { await refreshTasks() }
    }
}
