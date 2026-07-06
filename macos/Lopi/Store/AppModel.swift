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
    /// Loop Engineering snapshot for the Loop screen (nil until first fetch).
    var loopSnapshot: LoopSnapshot?
    var loopHealth: LoopHealth?
    var loopRuns: [LoopRun] = []
    var selectedRun: String?
    var loopTrace: LoopRunTrace?
    var traceLoading = false
    /// Launch-control dropdown sources, fetched from the server (sandbox-safe).
    var repos: [String] = []
    var branches: [String] = []
    /// The selected repo's default (current HEAD) branch.
    var defaultBranch: String = ""

    /// Rolling buffer of recent live log lines (most recent last), capped.
    var recentLogs: [String] = []

    // MARK: Live cockpit state (assembled from the event stream)

    /// Per-task live cognition, keyed by task id.
    var liveAgents: [String: LiveAgent] = [:]
    /// Newest-first rolling event ticker, capped.
    var feed: [FeedItem] = []
    /// Rolling samples of fleet cost (USD) for the dashboard sparkline.
    var costSeries: [Double] = []
    /// Rolling samples of aggregate tokens/sec across active agents.
    var throughputSeries: [Double] = []
    /// Mean generation activity across active agents, 0...1 — drives the
    /// intensity of the animated background.
    var aggregateActivity: Double = 0
    /// Most recent budget breach, if any (cleared by the UI).
    var lastBudget: BudgetBreach?

    /// Active agents, most-recently-updated first.
    var activeAgents: [LiveAgent] {
        liveAgents.values
            .sorted { $0.lastUpdate > $1.lastUpdate }
    }

    /// Non-fatal error banner text (auto-cleared by the UI).
    var banner: String?

    /// REST client — readable by the admin extension; reassigned only here
    /// when the server config changes.
    @ObservationIgnored private(set) var client: LopiClient
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

    /// Fetch the Loop Engineering snapshot + health + run list for the Loop
    /// screen. All three are independent reads — fetch them concurrently.
    func refreshLoop() async {
        do {
            async let snap = client.loopEngineering()
            async let health = client.loopHealth()
            async let runs = client.loopRuns()
            loopSnapshot = try await snap
            loopHealth = try await health
            loopRuns = try await runs
        } catch { report(error) }
    }

    /// Toggle a run's drill-down trace: select + fetch, or collapse if re-tapped.
    func selectRun(_ id: String) async {
        if selectedRun == id {
            selectedRun = nil
            loopTrace = nil
            return
        }
        selectedRun = id
        loopTrace = nil
        traceLoading = true
        do { loopTrace = try await client.loopRunTrace(id: id) } catch { report(error) }
        traceLoading = false
    }

    /// Set a scheduled loop's trust (autonomy) level, then re-pull the snapshot.
    func setScheduleAutonomy(_ id: String, level: String) async {
        do {
            try await client.setScheduleAutonomy(id: id, level: level)
            await refreshLoop()
        } catch { report(error) }
    }

    /// Set the repo's self-prompting strategy (persisted to `.lopi/loop.toml`),
    /// then re-pull the snapshot so the UI reflects the saved value.
    func setLoopStrategy(_ strategy: String) async {
        do {
            try await client.setLoopStrategy(strategy: strategy)
            await refreshLoop()
        } catch { report(error) }
    }

    /// Toggle adaptive strategy escalation (persisted to `.lopi/loop.toml`),
    /// then re-pull the snapshot.
    func setLoopEscalation(_ enabled: Bool) async {
        do {
            try await client.setLoopEscalation(enabled: enabled)
            await refreshLoop()
        } catch { report(error) }
    }

    /// Best-effort dropdown population — silent on failure (the field just
    /// stays empty / falls back to free entry).
    func refreshRepos() async {
        if let r = try? await client.repos() { repos = r }
    }

    func refreshBranches(_ repo: String) async {
        if let r = try? await client.branches(repo: repo) {
            branches = r.branches
            defaultBranch = r.defaultBranch
        } else {
            branches = []
            defaultBranch = ""
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

    /// Phase 11 — approve (proceed) or reject (abandon) a paused plan.
    func decidePlan(_ id: String, approve: Bool) async {
        do {
            try await client.decidePlan(id: id, approve: approve)
            // Optimistically clear the local gate; the WS status will confirm.
            liveAgents[id]?.awaitingApproval = false
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
                    Task { @MainActor in self?.ingest(event) }
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
        // Seed live agents from the snapshot task list so the cockpit isn't
        // empty between connect and the first live event.
        if let taskList = obj["tasks"] as? [[String: Any]] {
            for t in taskList {
                guard let id = t["id"] as? String else { continue }
                seedAgent(id: id, goal: t["goal"] as? String ?? "", phase: TaskStatusLabel.from(t["status"]))
            }
        }
        Task { await refreshTasks() }
    }

    /// Surface a non-fatal error in the banner. Internal so the admin
    /// extension can reuse the same reporting path.
    func report(_ error: Error) {
        banner = (error as? LopiError)?.errorDescription ?? error.localizedDescription
    }
}
