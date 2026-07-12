import Foundation

// Admin-surface endpoints still carried by the native app: the effective
// server config tree and the result cache. The dead-letter queue, audit log,
// patterns, tools, and per-agent health panels were removed in
// macOS-Parity-Cut-1 to match web, which no longer surfaces them.
extension LopiClient {
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

private struct ConfigWrapper: Decodable {
    let config: JSONValue?
    let source: String
}
