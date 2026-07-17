import Foundation

// MAXX (opportunistic backlog dispatch) — mirrors web `$lib/api.ts`'s
// `MaxxEntry`/`MaxxBody`/`QuotaSnapshot` types and `/api/maxx`, `/api/quota`
// endpoints. Rust source of truth: `crates/lopi-ui/src/web/maxx_handlers.rs`.

/// One run fired by a MAXX entry.
struct MaxxEntryRun: Codable, Hashable {
    let id: String
    let maxxId: String
    let firedAt: String
    let taskId: String?
    let outcome: String

    enum CodingKeys: String, CodingKey {
        case id
        case maxxId = "maxx_id"
        case firedAt = "fired_at"
        case taskId = "task_id"
        case outcome
    }
}

/// A MAXX backlog-dispatch entry as returned by `/api/maxx`.
struct MaxxEntry: Codable, Identifiable, Hashable {
    let id: String
    var name: String
    var goal: String
    var repo: String?
    var priority: String?
    var enabled: Bool
    var autonomyLevel: String
    var report: String?
    var quietHours: [Int]?
    var headroomGate: Bool
    var windows: [String]
    var createdAt: String
    var updatedAt: String
    var lastRun: MaxxEntryRun?
    var runs: [MaxxEntryRun]?

    enum CodingKeys: String, CodingKey {
        case id, name, goal, repo, priority, enabled, report, windows
        case autonomyLevel = "autonomy_level"
        case quietHours = "quiet_hours"
        case headroomGate = "headroom_gate"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
        case lastRun = "last_run"
        case runs
    }
}

/// The create/update body for `/api/maxx`.
struct MaxxBody: Encodable {
    var name: String
    var goal: String
    var repo: String?
    var priority: String?
    var enabled: Bool?
    var autonomyLevel: String?
    var report: String?
    var quietHours: [Int]?
    var headroomGate: Bool?
    var windows: [String]?

    enum CodingKeys: String, CodingKey {
        case name, goal, repo, priority, enabled, report, windows
        case autonomyLevel = "autonomy_level"
        case quietHours = "quiet_hours"
        case headroomGate = "headroom_gate"
    }
}

/// One Anthropic account rate-limit window's current utilization, from
/// `GET /api/quota`.
struct QuotaWindow: Codable, Hashable {
    let status: String
    /// 0...1.
    let utilization: Double
    /// Unix seconds, `nil` if unknown.
    let resetsAt: Int?
    let observedAt: String

    enum CodingKeys: String, CodingKey {
        case status, utilization
        case resetsAt = "resets_at"
        case observedAt = "observed_at"
    }
}

/// `GET /api/quota` — the 5h/7d window snapshot MAXX's headroom gate reads.
struct QuotaSnapshot: Codable, Hashable {
    let fiveHour: QuotaWindow?
    let sevenDay: QuotaWindow?

    enum CodingKeys: String, CodingKey {
        case fiveHour = "five_hour"
        case sevenDay = "seven_day"
    }
}

extension LopiClient {
    // MARK: MAXX CRUD

    func listMaxx() async throws -> [MaxxEntry] {
        struct Wrapper: Decodable { let maxx: [MaxxEntry] }
        let w: Wrapper = try await get("/api/maxx")
        return w.maxx
    }

    @discardableResult
    func createMaxx(_ body: MaxxBody) async throws -> MaxxEntry {
        try await sendDecoding("POST", "/api/maxx", body: body)
    }

    @discardableResult
    func updateMaxx(id: String, _ body: MaxxBody) async throws -> MaxxEntry {
        try await sendDecoding("PUT", "/api/maxx/\(id)", body: body)
    }

    func deleteMaxx(id: String) async throws {
        _ = try await send("DELETE", "/api/maxx/\(id)", body: Optional<Int>.none)
    }

    func enableMaxx(id: String) async throws {
        _ = try await send("POST", "/api/maxx/\(id)/enable", body: Optional<Int>.none)
    }

    func disableMaxx(id: String) async throws {
        _ = try await send("POST", "/api/maxx/\(id)/disable", body: Optional<Int>.none)
    }

    // MARK: Quota

    func quota() async throws -> QuotaSnapshot {
        try await get("/api/quota")
    }
}
