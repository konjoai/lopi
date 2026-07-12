import Foundation

// Admin-surface operations. Reads return data (empty on failure, with the
// error surfaced in the banner); mutations report success via return value
// so callers can refresh their local state.
extension AppModel {
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
