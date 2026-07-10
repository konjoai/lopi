import Foundation

// Admin-surface operations. Reads return data (empty on failure, with the
// error surfaced in the banner); mutations report success via return value
// so callers can refresh their local state.
extension AppModel {
    // MARK: Dead-letter queue

    func deadLetters() async -> [DeadLetter] {
        await fetch { try await self.client.deadLetters() } ?? []
    }

    func retryDeadLetter(_ id: String) async -> Bool {
        await mutate { try await self.client.retryDeadLetter(id: id) }
    }

    func discardDeadLetter(_ id: String) async -> Bool {
        await mutate { try await self.client.deleteDeadLetter(id: id) }
    }

    // MARK: Audit

    func audit(sinceId: Int, action: String?) async -> (entries: [AuditEntry], nextCursor: Int) {
        await fetch { try await self.client.audit(sinceId: sinceId, action: action) }
            ?? ([], sinceId)
    }

    // MARK: Patterns / health

    func patterns() async -> [PatternModel] {
        await fetch { try await self.client.patterns() } ?? []
    }

    func healthSummary() async -> HealthSummary? {
        await fetch { try await self.client.healthSummary() }
    }

    // MARK: Tools

    func tools() async -> [ToolModel] {
        await fetch { try await self.client.tools() } ?? []
    }

    func registerTool(_ body: RegisterToolBody) async -> Bool {
        await mutate { try await self.client.registerTool(body) }
    }

    func deleteTool(_ name: String) async -> Bool {
        await mutate { try await self.client.deleteTool(name: name) }
    }

    // MARK: Config + cache

    func configTree() async -> (config: JSONValue, source: String)? {
        await fetch { try await self.client.configTree() }
    }

    func cacheStats() async -> CacheStatsModel? {
        await fetch { try await self.client.cacheStats() }
    }

    func clearCache() async -> Bool {
        await mutate { try await self.client.clearCache() }
    }

    // MARK: Shared plumbing

    private func fetch<T>(_ op: () async throws -> T) async -> T? {
        do {
            return try await op()
        } catch {
            report(error)
            return nil
        }
    }

    private func mutate(_ op: () async throws -> Void) async -> Bool {
        do {
            try await op()
            return true
        } catch {
            report(error)
            return false
        }
    }
}
