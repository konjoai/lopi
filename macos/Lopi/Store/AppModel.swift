import Foundation
import Observation

/// Single source of UI state. Owns the REST client and the live event stream,
/// and exposes everything the views render. Lives on the main actor.
@Observable
@MainActor
final class AppModel {
    // Connection
    var config: ServerConfig
    var connection: ConnectionState = .offline
    var serverVersion: ServerVersion?

    // Live state
    var stats = PoolStats()
    var tasks: [TaskSummary] = []
    var schedules: [Schedule] = []

    /// Rolling buffer of recent live log lines (most recent last), capped.
    var recentLogs: [String] = []

    /// Non-fatal error banner text (auto-cleared by the UI).
    var banner: String?

    @ObservationIgnored private var client: LopiClient
    @ObservationIgnored private let stream = EventStream()

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
        do { schedules = try await client.schedules() } catch { report(error) }
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
                    Task { @MainActor in self?.connection = state }
                },
                onSnapshot: { [weak self] obj in
                    Task { @MainActor in self?.applySnapshot(obj) }
                },
                onEvent: { [weak self] event in
                    Task { @MainActor in self?.apply(event) }
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
        case let .logLine(_, line, level):
            appendLog("[\(level)] \(line)")
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

    private func report(_ error: Error) {
        banner = (error as? LopiError)?.errorDescription ?? error.localizedDescription
    }
}
