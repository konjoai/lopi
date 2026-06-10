import Foundation

// Codable mirrors of the lopi-core wire types. Field names match the JSON the
// axum API emits; see crates/lopi-core/src/{task,event}.rs for the source of
// truth.

/// A task row as returned by `GET /api/tasks`.
struct TaskSummary: Codable, Identifiable, Hashable {
    let id: String
    let goal: String
    let status: String
    let createdAt: String?
    let completedAt: String?

    enum CodingKeys: String, CodingKey {
        case id, goal, status
        case createdAt = "created_at"
        case completedAt = "completed_at"
    }
}

/// Fleet-wide stats from `GET /api/stats`.
struct PoolStats: Codable, Hashable {
    var running: Int = 0
    var queued: Int = 0
    var succeeded: Int = 0
    var failed: Int = 0
    var uptimeSecs: Int = 0
    var totalTokensToday: Int = 0
    var totalCostUsdToday: Double = 0

    enum CodingKeys: String, CodingKey {
        case running, queued, succeeded, failed
        case uptimeSecs = "uptime_secs"
        case totalTokensToday = "total_tokens_today"
        case totalCostUsdToday = "total_cost_usd_today"
    }
}

/// Server identity from `GET /api/version`.
struct ServerVersion: Codable, Hashable {
    let service: String
    let version: String
    let uptimeSecs: Int

    enum CodingKeys: String, CodingKey {
        case service, version
        case uptimeSecs = "uptime_secs"
    }
}

/// A single log line from `GET /api/tasks/:id/logs`.
struct TaskLog: Codable, Identifiable, Hashable {
    let id: Int
    let taskId: String
    let ts: String
    let level: String
    let line: String

    enum CodingKeys: String, CodingKey {
        case id, ts, level, line
        case taskId = "task_id"
    }
}

// MARK: - Schedules (cron)

/// A cron schedule as returned by `/api/schedules`.
struct Schedule: Codable, Identifiable, Hashable {
    let id: String
    var name: String
    var cron: String
    var goal: String
    var repo: String?
    var priority: String
    var allowedDirs: [String]
    var forbiddenDirs: [String]
    var enabled: Bool
    var createdAt: String?
    var updatedAt: String?
    var nextRuns: [String]?
    var lastRun: ScheduleRun?

    enum CodingKeys: String, CodingKey {
        case id, name, cron, goal, repo, priority, enabled
        case allowedDirs = "allowed_dirs"
        case forbiddenDirs = "forbidden_dirs"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
        case nextRuns = "next_runs"
        case lastRun = "last_run"
    }
}

/// One row of a schedule's run history.
struct ScheduleRun: Codable, Identifiable, Hashable {
    let id: String
    let scheduleId: String
    let firedAt: String
    let taskId: String?
    let outcome: String

    enum CodingKeys: String, CodingKey {
        case id, outcome
        case scheduleId = "schedule_id"
        case firedAt = "fired_at"
        case taskId = "task_id"
    }
}

/// Body for creating/updating a schedule.
struct ScheduleBody: Codable {
    var name: String
    var cron: String
    var goal: String
    var repo: String?
    var priority: String?
    var allowedDirs: [String]?
    var forbiddenDirs: [String]?
    var enabled: Bool?

    enum CodingKeys: String, CodingKey {
        case name, cron, goal, repo, priority, enabled
        case allowedDirs = "allowed_dirs"
        case forbiddenDirs = "forbidden_dirs"
    }
}

// MARK: - Task creation

/// Body for `POST /api/tasks`.
struct CreateTaskBody: Codable {
    var goal: String
    var repo: String?
    var priority: String?
    var constraints: [String]?
    var allowedDirs: [String]?
    var forbiddenDirs: [String]?
    var maxRetries: Int?

    enum CodingKeys: String, CodingKey {
        case goal, repo, priority, constraints
        case allowedDirs = "allowed_dirs"
        case forbiddenDirs = "forbidden_dirs"
        case maxRetries = "max_retries"
    }
}

// MARK: - Live events

/// A decoded `AgentEvent`. The Rust enum is internally tagged
/// (`#[serde(tag = "type", rename_all = "snake_case")]`); we decode the cases
/// the UI reacts to and ignore the rest.
enum AgentEvent {
    case taskQueued(taskId: String, goal: String)
    case taskStarted(taskId: String)
    case statusChanged(taskId: String, status: String)
    case logLine(taskId: String, line: String, level: String)
    case taskCompleted(taskId: String, outcome: String)
    case taskCancelled(taskId: String)
    case poolStats(PoolStats)
    case budgetExceeded(scope: String, limitUsd: Double, burnedUsd: Double)
    case other(type: String)

    /// Parse one event from raw JSON; returns nil if the payload is malformed.
    static func decode(from data: Data) -> AgentEvent? {
        guard
            let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
            let type = obj["type"] as? String
        else { return nil }

        switch type {
        case "task_queued":
            return .taskQueued(taskId: str(obj["task_id"]), goal: obj["goal"] as? String ?? "")
        case "task_started":
            return .taskStarted(taskId: str(obj["task_id"]))
        case "status_changed":
            return .statusChanged(taskId: str(obj["task_id"]), status: TaskStatusLabel.from(obj["status"]))
        case "log_line":
            return .logLine(
                taskId: str(obj["task_id"]),
                line: obj["line"] as? String ?? "",
                level: obj["level"] as? String ?? "info"
            )
        case "task_completed":
            return .taskCompleted(taskId: str(obj["task_id"]), outcome: TaskStatusLabel.from(obj["outcome"]))
        case "task_cancelled":
            return .taskCancelled(taskId: str(obj["task_id"]))
        case "pool_stats":
            if let data = try? JSONSerialization.data(withJSONObject: obj),
               let stats = try? JSONDecoder().decode(PoolStats.self, from: data) {
                return .poolStats(stats)
            }
            return .other(type: type)
        case "budget_exceeded":
            return .budgetExceeded(
                scope: obj["scope"] as? String ?? "fleet",
                limitUsd: obj["limit_usd"] as? Double ?? 0,
                burnedUsd: obj["burned_usd"] as? Double ?? 0
            )
        default:
            return .other(type: type)
        }
    }

    private static func str(_ v: Any?) -> String { v as? String ?? "" }
}

/// Renders a `TaskStatus` (string or single-key object) into a short label.
enum TaskStatusLabel {
    static func from(_ value: Any?) -> String {
        if let s = value as? String { return s } // unit variant: "Queued", …
        if let obj = value as? [String: Any], let key = obj.keys.first {
            return key // struct variant: {"Success": {…}} → "Success"
        }
        return "Unknown"
    }
}
