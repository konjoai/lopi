import Foundation

// Typed mirrors of the admin-surface JSON shapes. Sources of truth:
// dlq_handlers.rs, audit_handlers.rs, handlers.rs (patterns), tools registry,
// health_handlers.rs, result_cache.rs.

/// One dead-letter row from `GET /api/tasks/dead-letter`.
struct DeadLetter: Codable, Identifiable, Hashable {
    let id: String
    let taskId: String
    let goal: String
    let repoPath: String?
    let totalAttempts: Int
    let lastError: String?
    let firstFailedAt: String
    let deadAt: String
    let source: String

    enum CodingKeys: String, CodingKey {
        case id, goal, source
        case taskId = "task_id"
        case repoPath = "repo_path"
        case totalAttempts = "total_attempts"
        case lastError = "last_error"
        case firstFailedAt = "first_failed_at"
        case deadAt = "dead_at"
    }
}

/// One audit-log row from `GET /api/audit`.
struct AuditEntry: Codable, Identifiable, Hashable {
    let id: Int
    let ts: String
    let action: String
    let subjectType: String?
    let subjectId: String?
    let actor: String?
    let payload: String?

    enum CodingKeys: String, CodingKey {
        case id, ts, action, actor, payload
        case subjectType = "subject_type"
        case subjectId = "subject_id"
    }
}

/// One mined pattern from `GET /api/patterns`.
struct PatternModel: Codable, Identifiable, Hashable {
    let id: String
    let goalKeywords: String
    let avgAttempts: Double?
    let successRate: Double?
    let lastSeen: String

    enum CodingKeys: String, CodingKey {
        case id
        case goalKeywords = "goal_keywords"
        case avgAttempts = "avg_attempts"
        case successRate = "success_rate"
        case lastSeen = "last_seen"
    }
}

/// One registered tool from `GET /api/tools` (lopi-tools `ToolSpec`).
struct ToolModel: Codable, Identifiable, Hashable {
    var id: String { name }
    let name: String
    let description: String
    let parameters: JSONValue
    let timeoutMs: Int
    let retries: Int
    let updatedAt: String

    enum CodingKeys: String, CodingKey {
        case name, description, parameters, retries
        case timeoutMs = "timeout_ms"
        case updatedAt = "updated_at"
    }
}

/// Body for `POST /api/tools`.
struct RegisterToolBody: Codable {
    var name: String
    var description: String
    var parameters: JSONValue
    var timeoutMs: Int = 30_000
    var retries: Int = 0

    enum CodingKeys: String, CodingKey {
        case name, description, parameters, retries
        case timeoutMs = "timeout_ms"
    }
}

/// Fleet health rollup from `GET /api/agents/health/summary`.
struct HealthSummary: Codable, Hashable {
    let total: Int
    let healthy: Int
    let degraded: Int
    let dead: Int
}

/// Result-cache stats from `GET /api/cache/stats`.
struct CacheStatsModel: Codable, Hashable {
    let totalEntries: Int
    let totalSizeBytes: Int
    let hitRateLastHour: Double
    let oldestEntry: String?

    enum CodingKeys: String, CodingKey {
        case totalEntries = "total_entries"
        case totalSizeBytes = "total_size_bytes"
        case hitRateLastHour = "hit_rate_last_hour"
        case oldestEntry = "oldest_entry"
    }
}

// MARK: - Arbitrary JSON

/// A Codable representation of arbitrary JSON — used for tool parameter
/// schemas and the server config tree, where the shape is open-ended.
indirect enum JSONValue: Codable, Hashable {
    case null
    case bool(Bool)
    case number(Double)
    case string(String)
    case array([JSONValue])
    case object([String: JSONValue])

    init(from decoder: Decoder) throws {
        let c = try decoder.singleValueContainer()
        if c.decodeNil() {
            self = .null
        } else if let b = try? c.decode(Bool.self) {
            self = .bool(b)
        } else if let n = try? c.decode(Double.self) {
            self = .number(n)
        } else if let s = try? c.decode(String.self) {
            self = .string(s)
        } else if let a = try? c.decode([JSONValue].self) {
            self = .array(a)
        } else if let o = try? c.decode([String: JSONValue].self) {
            self = .object(o)
        } else {
            throw DecodingError.dataCorruptedError(in: c, debugDescription: "unsupported JSON value")
        }
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.singleValueContainer()
        switch self {
        case .null: try c.encodeNil()
        case let .bool(b): try c.encode(b)
        case let .number(n): try c.encode(n)
        case let .string(s): try c.encode(s)
        case let .array(a): try c.encode(a)
        case let .object(o): try c.encode(o)
        }
    }

    /// Multi-line, indented rendering for read-only JSON panels.
    func pretty(indent: Int = 0) -> String {
        let pad = String(repeating: "  ", count: indent)
        let inner = String(repeating: "  ", count: indent + 1)
        switch self {
        case .null: return "null"
        case let .bool(b): return b ? "true" : "false"
        case let .number(n):
            return n == n.rounded() && abs(n) < 1e15
                ? String(Int(n))
                : String(n)
        case let .string(s): return "\"\(s)\""
        case let .array(a):
            if a.isEmpty { return "[]" }
            let items = a.map { "\(inner)\($0.pretty(indent: indent + 1))" }
            return "[\n" + items.joined(separator: ",\n") + "\n\(pad)]"
        case let .object(o):
            if o.isEmpty { return "{}" }
            let items = o.keys.sorted().map { key in
                "\(inner)\"\(key)\": \(o[key, default: .null].pretty(indent: indent + 1))"
            }
            return "{\n" + items.joined(separator: ",\n") + "\n\(pad)}"
        }
    }
}
