import Foundation

// Typed mirrors of the admin-surface JSON shapes still consumed by the native
// app: the result cache and (via JSONValue) the server config tree. The
// dead-letter, audit, patterns, tools, and health shapes were removed in
// macOS-Parity-Cut-1 alongside their panels. Source of truth: result_cache.rs.

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
