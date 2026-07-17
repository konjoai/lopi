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
///
/// Decoding is lenient: the live `/ws` `pool_stats` event and the snapshot's
/// `stats` object carry only the five counter fields — the token/cost totals
/// exist only in the REST response. Missing keys default to 0 so live frames
/// decode instead of failing wholesale.
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

    init() {}

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        running = try c.decodeIfPresent(Int.self, forKey: .running) ?? 0
        queued = try c.decodeIfPresent(Int.self, forKey: .queued) ?? 0
        succeeded = try c.decodeIfPresent(Int.self, forKey: .succeeded) ?? 0
        failed = try c.decodeIfPresent(Int.self, forKey: .failed) ?? 0
        uptimeSecs = try c.decodeIfPresent(Int.self, forKey: .uptimeSecs) ?? 0
        totalTokensToday = try c.decodeIfPresent(Int.self, forKey: .totalTokensToday) ?? 0
        totalCostUsdToday = try c.decodeIfPresent(Double.self, forKey: .totalCostUsdToday) ?? 0
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

// MARK: - Schedule chains (whole-stack cron)

/// Stack-Chain-1 — sibling to `Schedule`, but a chain carries an ORDERED
/// SEQUENCE of goals (one per stack card) instead of a single `goal`.
/// Mirrors `web/src/lib/api.ts`'s `ScheduleChain`.
struct ScheduleChainStep: Codable, Hashable {
    var stepOrder: Int
    var goal: String
    var allowedDirs: [String]
    var forbiddenDirs: [String]

    enum CodingKeys: String, CodingKey {
        case goal
        case stepOrder = "step_order"
        case allowedDirs = "allowed_dirs"
        case forbiddenDirs = "forbidden_dirs"
    }
}

struct ScheduleChainRun: Codable, Identifiable, Hashable {
    let id: String
    let chainId: String
    let firedAt: String
    let currentStep: Int
    let currentTaskId: String?
    let status: String

    enum CodingKeys: String, CodingKey {
        case id, status
        case chainId = "chain_id"
        case firedAt = "fired_at"
        case currentStep = "current_step"
        case currentTaskId = "current_task_id"
    }
}

struct ScheduleChain: Codable, Identifiable, Hashable {
    let id: String
    var name: String
    var cron: String
    var repo: String?
    var priority: String?
    var autonomyLevel: String
    var onFail: String
    var enabled: Bool
    var steps: [ScheduleChainStep]
    var createdAt: String?
    var updatedAt: String?
    var nextRuns: [String]?
    var lastRun: ScheduleChainRun?

    enum CodingKeys: String, CodingKey {
        case id, name, cron, repo, priority, enabled, steps
        case autonomyLevel = "autonomy_level"
        case onFail = "on_fail"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
        case nextRuns = "next_runs"
        case lastRun = "last_run"
    }
}

/// Body for creating/updating a schedule chain.
struct ScheduleChainStepBody: Codable {
    var goal: String
    var allowedDirs: [String]?
    var forbiddenDirs: [String]?

    enum CodingKeys: String, CodingKey {
        case goal
        case allowedDirs = "allowed_dirs"
        case forbiddenDirs = "forbidden_dirs"
    }
}

struct ScheduleChainBody: Codable {
    var name: String
    var cron: String
    var steps: [ScheduleChainStepBody]
    var repo: String?
    var priority: String?
    var autonomyLevel: String?
    var onFail: String?
    var enabled: Bool?

    enum CodingKeys: String, CodingKey {
        case name, cron, steps, repo, priority, enabled
        case autonomyLevel = "autonomy_level"
        case onFail = "on_fail"
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
    /// Worker-model override. Sent as the real `model` field the backend's
    /// `select_model` honors verbatim — previously the picked model was folded
    /// into a free-text constraint the runner ignored, so it fell back to the
    /// complexity heuristic and the running model never matched the label the
    /// pane showed (Ops-2 finding #7).
    var model: String? = nil
    /// Reasoning-effort hint. Real `effort` field, same rationale as `model`.
    var effort: String? = nil
    // ── Loop-stack WIRED fields (macOS Loop Stacks) ──────────────────────────
    // Real `CreateTaskRequest` fields the backend honors (landed PR #62 / A3).
    // Additive + optional, so they encode only when a stack card sets them and
    // every existing bare-prompt call site (LaunchControls.body) still compiles
    // unchanged. `budget_tokens`/`acceptance` are deliberately NOT mapped here:
    // acceptance is A1–B1's evaluator track (out of scope, "no backend changes"),
    // and budget_tokens has no request field yet — same honesty gap as web.
    /// Hard per-loop iteration ceiling (`0` = infinite sentinel).
    var maxIterations: Int? = nil
    /// On-fail policy — `stop` / `continue` / `backoff`.
    var onFail: String? = nil
    /// Shell precondition that must pass before the loop runs.
    var gate: String? = nil
    /// Shell exit-condition the loop runs until.
    var until: String? = nil
    /// Client ref so the response's task id traces back to the launching card.
    var clientRef: String? = nil

    enum CodingKeys: String, CodingKey {
        case goal, repo, priority, constraints, model, effort, gate, until
        case allowedDirs = "allowed_dirs"
        case forbiddenDirs = "forbidden_dirs"
        case maxRetries = "max_retries"
        case maxIterations = "max_iterations"
        case onFail = "on_fail"
        case clientRef = "client_ref"
    }
}

// MARK: - Live events

/// A decoded `AgentEvent`. The Rust enum is internally tagged
/// (`#[serde(tag = "type", rename_all = "snake_case")]`); we decode the cases
/// the UI reacts to and ignore the rest.
enum AgentEvent {
    case taskQueued(taskId: String, goal: String, priority: String)
    case taskStarted(taskId: String, attempt: Int, branch: String)
    case statusChanged(taskId: String, status: String, attempt: Int)
    case planProposed(taskId: String, attempt: Int, steps: [String], plan: String)
    case logLine(taskId: String, line: String, level: String)
    case scoreUpdated(taskId: String, testPassRate: Double, lintErrors: Int, diffLines: Int)
    case turnMetrics(taskId: String, pressure: Double, activity: Double, tokensPerSec: Double, costUsd: Double)
    case verifierVerdict(taskId: String, passed: Bool, gaps: [String], confidence: Double)
    case taskCompleted(taskId: String, outcome: String, totalAttempts: Int)
    case taskCancelled(taskId: String)
    case poolStats(PoolStats)
    case budgetExceeded(taskId: String?, scope: String, limitUsd: Double, burnedUsd: Double)
    // stream-json pane events (Phase 1 event spine) — mirror lopi-core.
    case toolCall(taskId: String, tool: String, summary: String)
    case toolResult(taskId: String, tool: String, isError: Bool, preview: String)
    case tokenDelta(taskId: String, outputTokens: Int, inputTokens: Int, cacheReadTokens: Int)
    case apiRetry(taskId: String, status: String, limitType: String, utilization: Double)
    case cost(taskId: String, costUsd: Double, numTurns: Int, sessionId: String)
    case phase(taskId: String, phase: String)
    case other(type: String)

    /// The task id this event concerns, if any — used to route events to the
    /// per-agent live state.
    var taskId: String? {
        switch self {
        case let .taskQueued(id, _, _), let .taskStarted(id, _, _),
             let .statusChanged(id, _, _), let .planProposed(id, _, _, _),
             let .logLine(id, _, _),
             let .scoreUpdated(id, _, _, _), let .turnMetrics(id, _, _, _, _),
             let .verifierVerdict(id, _, _, _), let .taskCompleted(id, _, _),
             let .taskCancelled(id),
             let .toolCall(id, _, _), let .toolResult(id, _, _, _),
             let .tokenDelta(id, _, _, _), let .apiRetry(id, _, _, _),
             let .cost(id, _, _, _), let .phase(id, _):
            return id
        case let .budgetExceeded(id, _, _, _):
            return id
        case .poolStats, .other:
            return nil
        }
    }

    /// Parse one event from raw JSON; returns nil if the payload is malformed.
    static func decode(from data: Data) -> AgentEvent? {
        guard
            let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
            let type = obj["type"] as? String
        else { return nil }

        switch type {
        case "task_queued":
            return .taskQueued(
                taskId: str(obj["task_id"]),
                goal: obj["goal"] as? String ?? "",
                priority: TaskStatusLabel.from(obj["priority"])
            )
        case "task_started":
            return .taskStarted(
                taskId: str(obj["task_id"]),
                attempt: num(obj["attempt"]),
                branch: obj["branch"] as? String ?? ""
            )
        case "status_changed":
            return .statusChanged(
                taskId: str(obj["task_id"]),
                status: TaskStatusLabel.from(obj["status"]),
                attempt: num(obj["attempt"])
            )
        case "plan_proposed":
            return .planProposed(
                taskId: str(obj["task_id"]),
                attempt: num(obj["attempt"]),
                steps: (obj["steps"] as? [Any])?.compactMap { $0 as? String } ?? [],
                plan: obj["plan"] as? String ?? ""
            )
        case "log_line":
            return .logLine(
                taskId: str(obj["task_id"]),
                line: obj["line"] as? String ?? "",
                level: obj["level"] as? String ?? "info"
            )
        case "score_updated":
            return .scoreUpdated(
                taskId: str(obj["task_id"]),
                testPassRate: dbl(obj["test_pass_rate"]),
                lintErrors: num(obj["lint_errors"]),
                diffLines: num(obj["diff_lines"])
            )
        case "turn_metrics":
            return .turnMetrics(
                taskId: str(obj["task_id"]),
                pressure: dbl(obj["pressure"]),
                activity: dbl(obj["activity"]),
                tokensPerSec: dbl(obj["tokens_per_sec"]),
                costUsd: dbl(obj["cost_usd"])
            )
        case "verifier_verdict":
            return .verifierVerdict(
                taskId: str(obj["task_id"]),
                passed: obj["passed"] as? Bool ?? false,
                gaps: obj["gaps"] as? [String] ?? [],
                confidence: dbl(obj["confidence"])
            )
        case "task_completed":
            return .taskCompleted(
                taskId: str(obj["task_id"]),
                outcome: TaskStatusLabel.from(obj["outcome"]),
                totalAttempts: num(obj["total_attempts"])
            )
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
                taskId: obj["task_id"] as? String,
                scope: TaskStatusLabel.from(obj["scope"]),
                limitUsd: dbl(obj["limit_usd"]),
                burnedUsd: dbl(obj["burned_usd"])
            )
        case "tool_call":
            return .toolCall(
                taskId: str(obj["task_id"]),
                tool: str(obj["tool"]),
                summary: str(obj["summary"])
            )
        case "tool_result":
            return .toolResult(
                taskId: str(obj["task_id"]),
                tool: str(obj["tool"]),
                isError: obj["is_error"] as? Bool ?? false,
                preview: str(obj["preview"])
            )
        case "token_delta":
            return .tokenDelta(
                taskId: str(obj["task_id"]),
                outputTokens: num(obj["output_tokens"]),
                inputTokens: num(obj["input_tokens"]),
                cacheReadTokens: num(obj["cache_read_tokens"])
            )
        case "api_retry":
            return .apiRetry(
                taskId: str(obj["task_id"]),
                status: str(obj["status"]),
                limitType: str(obj["limit_type"]),
                utilization: dbl(obj["utilization"])
            )
        case "cost":
            return .cost(
                taskId: str(obj["task_id"]),
                costUsd: dbl(obj["cost_usd"]),
                numTurns: num(obj["num_turns"]),
                sessionId: str(obj["session_id"])
            )
        case "phase":
            return .phase(
                taskId: str(obj["task_id"]),
                phase: str(obj["phase"])
            )
        default:
            return .other(type: type)
        }
    }

    private static func str(_ v: Any?) -> String { v as? String ?? "" }
    private static func num(_ v: Any?) -> Int { (v as? NSNumber)?.intValue ?? 0 }
    private static func dbl(_ v: Any?) -> Double { (v as? NSNumber)?.doubleValue ?? 0 }
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
