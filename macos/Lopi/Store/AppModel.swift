import Foundation
import Combine

/// Single source of UI state. Owns the REST client and the live event stream,
/// and exposes everything the views render. Lives on the main actor.
///
/// Uses `ObservableObject`/`@Published` (rather than the macOS 14-only
/// Observation `@Observable`) so the app runs on macOS 13 (Ventura) as well.
@MainActor
final class AppModel: ObservableObject {
    // Connection
    @Published var config: ServerConfig
    @Published var connection: ConnectionState = .offline
    @Published var serverVersion: ServerVersion?

    // Live state
    @Published var stats = PoolStats()
    @Published var tasks: [TaskSummary] = []
    @Published var schedules: [Schedule] = []

    /// Rolling buffer of recent live log lines (most recent last), capped.
    @Published var recentLogs: [String] = []

    /// Per-task live state derived from the event stream (last log line +
    /// recency), keyed by task id. Drives each pane's log tail and orb pulse.
    @Published var live: [String: LiveTask] = [:]

    /// Non-fatal error banner text (auto-cleared by the UI).
    @Published var banner: String?

    private var client: LopiClient
    private let stream = EventStream()

    init(config: ServerConfig = .load()) {
        self.config = config
        self.client = LopiClient(config: config)
    }

    // MARK: Lifecycle

    /// Connect the live stream and do an initial REST refresh.
    func start() {
        connectStream()
        Task { await refreshAll() }
    }

    /// Apply new server settings: persist, rewire client, reconnect.
    func updateConfig(_ new: ServerConfig) {
        new.save()
        config = new
        client = LopiClient(config: new)
        Task {
            await stream.stop()
            connectStream()
            await refreshAll()
        }
    }

    // MARK: REST refresh

    func refreshAll() async {
        await refreshVersion()
        await refreshStats()
        await refreshTasks()
        await refreshSchedules()
    }

    func refreshVersion() async {
        serverVersion = try? await client.version()
    }

    func refreshStats() async {
        if let s = try? await client.stats() { stats = s }
    }

    func refreshTasks() async {
        do { tasks = try await client.tasks() } catch { report(error) }
    }

    func refreshSchedules() async {
        do {
            schedules = try await client.schedules()
        } catch LopiError.unsupported {
            schedules = [] // server build lacks the cron API — not an error
        } catch {
            report(error)
        }
    }

    // MARK: Mutations

    func submitTask(_ body: CreateTaskBody) async {
        do {
            try await client.createTask(body)
            await refreshTasks()
        } catch { report(error) }
    }

    func cancelTask(_ id: String) async {
        do {
            try await client.cancelTask(id: id)
            await refreshTasks()
        } catch { report(error) }
    }

    func saveSchedule(id: String?, _ body: ScheduleBody) async {
        do {
            if let id {
                _ = try await client.updateSchedule(id: id, body)
            } else {
                _ = try await client.createSchedule(body)
            }
            await refreshSchedules()
        } catch { report(error) }
    }

    func toggleSchedule(_ schedule: Schedule) async {
        do {
            try await client.setScheduleEnabled(id: schedule.id, enabled: !schedule.enabled)
            await refreshSchedules()
        } catch { report(error) }
    }

    func runScheduleNow(_ schedule: Schedule) async {
        do {
            try await client.runScheduleNow(id: schedule.id)
            banner = "Triggered \(schedule.name)"
        } catch { report(error) }
    }

    func deleteSchedule(_ schedule: Schedule) async {
        do {
            try await client.deleteSchedule(id: schedule.id)
            await refreshSchedules()
        } catch { report(error) }
    }

    func logs(for taskId: String) async -> [TaskLog] {
        (try? await client.logs(taskId: taskId)) ?? []
    }

    // MARK: Live stream wiring

    private func connectStream() {
        guard let url = config.webSocketURL else { return }
        Task {
            await stream.setHandlers(
                onState: { [weak self] state in
                    Task { @MainActor [weak self] in self?.connection = state }
                },
                onSnapshot: { [weak self] obj in
                    Task { @MainActor [weak self] in self?.applySnapshot(obj) }
                },
                onEvent: { [weak self] event in
                    Task { @MainActor [weak self] in self?.apply(event) }
                }
            )
            await stream.start(url: url)
        }
    }

    private func applySnapshot(_ obj: [String: Any]) {
        if let statsObj = obj["stats"],
           let data = try? JSONSerialization.data(withJSONObject: statsObj),
           let s = try? JSONDecoder().decode(PoolStats.self, from: data) {
            stats = s
        }
        Task { await refreshTasks() }
    }

    private func apply(_ event: AgentEvent) {
        switch event {
        case let .poolStats(s):
            stats.running = s.running
            stats.queued = s.queued
            stats.succeeded = s.succeeded
            stats.failed = s.failed
            stats.uptimeSecs = s.uptimeSecs
        case let .logLine(taskId, line, level):
            appendLog("[\(level)] \(line)")
            noteLog(taskId: taskId, line: line, level: level)
        case .taskCompleted, .taskQueued, .statusChanged, .taskCancelled, .taskStarted:
            Task { await refreshTasks() }
        case let .budgetExceeded(scope, limit, burned):
            banner = String(format: "Budget exceeded (%@): $%.2f / $%.2f", scope, burned, limit)
        case .other:
            break
        }
    }

    private func appendLog(_ line: String) {
        recentLogs.append(line)
        if recentLogs.count > 500 { recentLogs.removeFirst(recentLogs.count - 500) }
    }

    /// Record the latest log line for a task, stamping the activity time so the
    /// pane orb can pulse while output is flowing.
    private func noteLog(taskId: String, line: String, level: String) {
        guard !taskId.isEmpty else { return }
        var lt = live[taskId] ?? LiveTask()
        lt.lastLine = line
        lt.lastLevel = level
        lt.lastActivityAt = Date()
        live[taskId] = lt
    }

    private func report(_ error: Error) {
        banner = (error as? LopiError)?.errorDescription ?? error.localizedDescription
    }
}

/// Live, stream-derived state for one task: the most recent log line and when
/// it arrived. `activity` decays from 1 → 0 over a few seconds of silence.
struct LiveTask: Equatable {
    var lastLine: String = ""
    var lastLevel: String = "info"
    var lastActivityAt: Date = .distantPast

    /// Recency-based generation intensity in 0…1 (1 = a log within the last
    /// moment, fading to 0 after ~8s of quiet).
    var activity: Double {
        let dt = Date().timeIntervalSince(lastActivityAt)
        return max(0, min(1, 1 - dt / 8))
    }
}
