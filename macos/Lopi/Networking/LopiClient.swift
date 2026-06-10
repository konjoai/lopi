import Foundation

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

    func logs(taskId: String, n: Int = 200) async throws -> [TaskLog] {
        let wrapper: LogsWrapper = try await get("/api/tasks/\(taskId)/logs?n=\(n)")
        return wrapper.logs
    }

    @discardableResult
    func createTask(_ body: CreateTaskBody) async throws -> Data {
        try await send("POST", "/api/tasks", body: body)
    }

    @discardableResult
    func cancelTask(id: String) async throws -> Data {
        try await send("DELETE", "/api/tasks/\(id)", body: Optional<Int>.none)
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

    // MARK: Core request machinery

    private func get<T: Decodable>(_ path: String) async throws -> T {
        let data = try await perform("GET", path, body: Optional<Int>.none)
        return try decode(data)
    }

    @discardableResult
    private func send<B: Encodable>(_ method: String, _ path: String, body: B?) async throws -> Data {
        try await perform(method, path, body: body)
    }

    private func sendDecoding<T: Decodable, B: Encodable>(
        _ method: String, _ path: String, body: B?
    ) async throws -> T {
        let data = try await perform(method, path, body: body)
        return try decode(data)
    }

    private func perform<B: Encodable>(_ method: String, _ path: String, body: B?) async throws -> Data {
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

    private func decode<T: Decodable>(_ data: Data) throws -> T {
        do {
            return try decoder.decode(T.self, from: data)
        } catch {
            throw LopiError.decoding(error.localizedDescription)
        }
    }

    private func message(_ data: Data) -> String {
        if let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
           let err = obj["error"] as? String {
            return err
        }
        return String(data: data, encoding: .utf8) ?? "unknown error"
    }
}

// Response envelopes the API wraps collections in.
private struct TasksWrapper: Decodable { let tasks: [TaskSummary] }
private struct LogsWrapper: Decodable { let logs: [TaskLog] }
private struct SchedulesWrapper: Decodable { let schedules: [Schedule] }
