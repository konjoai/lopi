import Foundation

// Admin-surface endpoints: dead-letter queue, audit log, patterns, tools,
// health, constellations, config, and the result cache.
extension LopiClient {
    // MARK: Dead-letter queue

    func deadLetters(n: Int = 100) async throws -> [DeadLetter] {
        let wrapper: DeadLettersWrapper = try await get("/api/tasks/dead-letter?n=\(n)")
        return wrapper.deadLetters
    }

    func retryDeadLetter(id: String) async throws {
        _ = try await send("POST", "/api/tasks/dead-letter/\(id)/retry", body: Optional<Int>.none)
    }

    func deleteDeadLetter(id: String) async throws {
        _ = try await send("DELETE", "/api/tasks/dead-letter/\(id)", body: Optional<Int>.none)
    }

    // MARK: Audit log

    /// Cursor-paginated audit query. Returns the page plus the next cursor.
    func audit(sinceId: Int = 0, action: String? = nil, n: Int = 100) async throws
        -> (entries: [AuditEntry], nextCursor: Int)
    {
        var path = "/api/audit?since_id=\(sinceId)&n=\(n)"
        if let action, !action.isEmpty,
           let escaped = action.addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed) {
            path += "&action=\(escaped)"
        }
        let wrapper: AuditWrapper = try await get(path)
        return (wrapper.events, wrapper.nextCursor)
    }

    // MARK: Patterns

    func patterns() async throws -> [PatternModel] {
        let wrapper: PatternsWrapper = try await get("/api/patterns")
        return wrapper.patterns
    }

    // MARK: Tools

    func tools() async throws -> [ToolModel] {
        let wrapper: ToolsWrapper = try await get("/api/tools")
        return wrapper.tools
    }

    func registerTool(_ body: RegisterToolBody) async throws {
        _ = try await send("POST", "/api/tools", body: body)
    }

    func deleteTool(name: String) async throws {
        _ = try await send("DELETE", "/api/tools/\(name)", body: Optional<Int>.none)
    }

    // MARK: Health

    func healthSummary() async throws -> HealthSummary {
        try await get("/api/agents/health/summary")
    }

    // MARK: Constellations

    func constellations() async throws -> [ConstellationModel] {
        let wrapper: ConstellationsWrapper = try await get("/api/constellations")
        return wrapper.constellations
    }

    // MARK: Config + cache

    /// The effective server config as an arbitrary JSON tree (secrets are
    /// redacted server-side). `config` is null when no lopi.toml was found.
    func configTree() async throws -> (config: JSONValue, source: String) {
        let wrapper: ConfigWrapper = try await get("/api/config")
        return (wrapper.config ?? .null, wrapper.source)
    }

    func cacheStats() async throws -> CacheStatsModel {
        try await get("/api/cache/stats")
    }

    func clearCache() async throws {
        _ = try await send("DELETE", "/api/cache", body: Optional<Int>.none)
    }
}

// Response envelopes for the admin endpoints.
private struct DeadLettersWrapper: Decodable {
    let deadLetters: [DeadLetter]
    enum CodingKeys: String, CodingKey { case deadLetters = "dead_letters" }
}

private struct AuditWrapper: Decodable {
    let events: [AuditEntry]
    let nextCursor: Int
    enum CodingKeys: String, CodingKey {
        case events
        case nextCursor = "next_cursor"
    }
}

private struct PatternsWrapper: Decodable { let patterns: [PatternModel] }
private struct ToolsWrapper: Decodable { let tools: [ToolModel] }
private struct ConstellationsWrapper: Decodable { let constellations: [ConstellationModel] }

private struct ConfigWrapper: Decodable {
    let config: JSONValue?
    let source: String
}
