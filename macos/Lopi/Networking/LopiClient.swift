import Foundation
import LopiStacksKit

/// Errors surfaced by `LopiClient`.
enum LopiError: LocalizedError {
    case badURL
    case unauthorized
    case rateLimited
    case http(status: Int, message: String)
    case decoding(String)
    case transport(String)

    var errorDescription: String? {
        switch self {
        case .badURL: return "Invalid server URL"
        case .unauthorized: return "Unauthorized — check the Bearer token in Settings"
        case .rateLimited: return "Rate limited — too many requests"
        case let .http(status, message): return "HTTP \(status): \(message)"
        case let .decoding(detail): return "Decoding error: \(detail)"
        case let .transport(detail): return "Network error: \(detail)"
        }
    }
}

/// Async REST client for the lopi API. Injects Bearer auth, retries `429`
/// responses with exponential backoff, and decodes typed responses.
struct LopiClient {
    var config: ServerConfig
    private let session: URLSession = .shared
    private let decoder: JSONDecoder = JSONDecoder()
    private let encoder: JSONEncoder = JSONEncoder()

    // Explicit init: the synthesized memberwise init inherits `private` from
    // the private stored properties above, which would make `LopiClient(config:)`
    // invisible outside this file.
    init(config: ServerConfig) {
        self.config = config
    }

    // MARK: Typed endpoints

    func version() async throws -> ServerVersion {
        try await get("/api/version")
    }

    func stats() async throws -> PoolStats {
        try await get("/api/stats")
    }

    func tasks() async throws -> [TaskSummary] {
        let wrapper: TasksWrapper = try await get("/api/tasks")
        return wrapper.tasks
    }


    @discardableResult
    func createTask(_ body: CreateTaskBody) async throws -> Data {
        try await send("POST", "/api/tasks", body: body)
    }

    @discardableResult
    func cancelTask(id: String) async throws -> Data {
        try await send("DELETE", "/api/tasks/\(id)", body: Optional<Int>.none)
    }

    /// Phase 11 — deliver a plan-approval decision.
    @discardableResult
    func decidePlan(id: String, approve: Bool) async throws -> Data {
        let verb = approve ? "approve" : "reject"
        return try await send("POST", "/api/tasks/\(id)/plan/\(verb)", body: Optional<Int>.none)
    }

    /// Git repos the server can target (primary + siblings + `--repos` extras),
    /// each with its GitHub `owner`/`name` for labelling.
    func repos() async throws -> [RepoEntry] {
        struct Wrapper: Decodable { let repos: [RepoEntry] }
        let w: Wrapper = try await get("/api/repos")
        return w.repos
    }

    /// Local branches of `repo` (empty → server's primary repo) plus the repo's
    /// default (current HEAD) branch.
    func branches(repo: String) async throws -> (branches: [String], defaultBranch: String) {
        struct Wrapper: Decodable {
            let branches: [String]
            let defaultBranch: String
            enum CodingKeys: String, CodingKey {
                case branches
                case defaultBranch = "default"
            }
        }
        let q = repo.addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed) ?? ""
        let w: Wrapper = try await get("/api/branches?repo=\(q)")
        return (w.branches, w.defaultBranch)
    }

    // Schedules

    func schedules() async throws -> [Schedule] {
        let wrapper: SchedulesWrapper = try await get("/api/schedules")
        return wrapper.schedules
    }

    func schedule(id: String) async throws -> Schedule {
        try await get("/api/schedules/\(id)")
    }

    @discardableResult
    func createSchedule(_ body: ScheduleBody) async throws -> Schedule {
        try await sendDecoding("POST", "/api/schedules", body: body)
    }

    @discardableResult
    func updateSchedule(id: String, _ body: ScheduleBody) async throws -> Schedule {
        try await sendDecoding("PUT", "/api/schedules/\(id)", body: body)
    }

    func setScheduleEnabled(id: String, enabled: Bool) async throws {
        let path = "/api/schedules/\(id)/\(enabled ? "enable" : "disable")"
        _ = try await send("POST", path, body: Optional<Int>.none)
    }

    func runScheduleNow(id: String) async throws {
        _ = try await send("POST", "/api/schedules/\(id)/run-now", body: Optional<Int>.none)
    }

    func deleteSchedule(id: String) async throws {
        _ = try await send("DELETE", "/api/schedules/\(id)", body: Optional<Int>.none)
    }

    // Loop Engineering

    func loopEngineering() async throws -> LoopSnapshot {
        try await get("/api/loop-engineering")
    }

    func loopHealth() async throws -> LoopHealth {
        try await get("/api/loop-engineering/health")
    }

    func loopRuns() async throws -> [LoopRun] {
        let list: LoopRunList = try await get("/api/loop-engineering/runs")
        return list.runs
    }

    func loopRunTrace(id: String) async throws -> LoopRunTrace {
        try await get("/api/loop-engineering/runs/\(id)")
    }

    func setScheduleAutonomy(id: String, level: String) async throws {
        _ = try await send("POST", "/api/schedules/\(id)/autonomy", body: ["level": level])
    }

    /// Set the repo's self-prompting strategy; the server persists it to
    /// `.lopi/loop.toml` (loop-as-code).
    func setLoopStrategy(strategy: String) async throws {
        _ = try await send("POST", "/api/loop-engineering/strategy", body: ["strategy": strategy])
    }

    /// Toggle adaptive strategy escalation; persisted to `.lopi/loop.toml`.
    func setLoopEscalation(enabled: Bool) async throws {
        _ = try await send("POST", "/api/loop-engineering/escalation", body: ["enabled": enabled])
    }

    // MARK: Core request machinery

    func get<T: Decodable>(_ path: String) async throws -> T {
        let data = try await perform("GET", path, body: Optional<Int>.none)
        return try decode(data)
    }

    @discardableResult
    func send<B: Encodable>(_ method: String, _ path: String, body: B?) async throws -> Data {
        try await perform(method, path, body: body)
    }

    func sendDecoding<T: Decodable, B: Encodable>(
        _ method: String, _ path: String, body: B?
    ) async throws -> T {
        let data = try await perform(method, path, body: body)
        return try decode(data)
    }

    func perform<B: Encodable>(_ method: String, _ path: String, body: B?) async throws -> Data {
        guard let base = config.baseURL, let url = URL(string: path, relativeTo: base) else {
            throw LopiError.badURL
        }
        var request = URLRequest(url: url)
        request.httpMethod = method
        if let token = config.token, !token.isEmpty {
            request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        }
        if let body {
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
            request.httpBody = try encoder.encode(body)
        }

        // Up to 4 attempts on 429, backing off 0.5s, 1s, 2s.
        var delay: UInt64 = 500_000_000
        for attempt in 0..<4 {
            do {
                let (data, response) = try await session.data(for: request)
                guard let http = response as? HTTPURLResponse else {
                    throw LopiError.transport("no HTTP response")
                }
                switch http.statusCode {
                case 200..<300:
                    return data
                case 401:
                    throw LopiError.unauthorized
                case 429 where attempt < 3:
                    try await Task.sleep(nanoseconds: delay)
                    delay *= 2
                    continue
                case 429:
                    throw LopiError.rateLimited
                default:
                    throw LopiError.http(status: http.statusCode, message: message(data))
                }
            } catch let error as LopiError {
                throw error
            } catch {
                throw LopiError.transport(error.localizedDescription)
            }
        }
        throw LopiError.rateLimited
    }

    func decode<T: Decodable>(_ data: Data) throws -> T {
        do {
            return try decoder.decode(T.self, from: data)
        } catch {
            throw LopiError.decoding(error.localizedDescription)
        }
    }

    func message(_ data: Data) -> String {
        if let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
           let err = obj["error"] as? String {
            return err
        }
        return String(data: data, encoding: .utf8) ?? "unknown error"
    }
}

// Response envelopes the API wraps collections in.
private struct TasksWrapper: Decodable { let tasks: [TaskSummary] }
private struct SchedulesWrapper: Decodable { let schedules: [Schedule] }
