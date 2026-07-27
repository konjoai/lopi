import Foundation
import Security

/// Connection settings for the lopi server. Host/port live in `UserDefaults`;
/// the Bearer token is stored in the macOS Keychain.
struct ServerConfig: Equatable {
    var host: String
    var port: Int
    var token: String?
    /// Explicit opt-in to allow cleartext `http`/`ws` to a *non-loopback*
    /// host. Ignored for loopback hosts (which always use `http`/`ws` — no
    /// regression for local dev). Defaults to `false`: a non-loopback host
    /// defaults to `https`/`wss` unless the operator explicitly opts out,
    /// mirroring the fail-closed spirit of `lopi-ui`'s
    /// `auth_policy::validate_auth_policy` (safe default, explicit opt-out).
    var allowInsecureHTTP: Bool = false

    /// Whether `host` is a loopback address. Mirrors `lopi-ui`'s
    /// `auth_policy::is_loopback_host`: the literal string `"localhost"`
    /// (case-insensitive), `::1`, or any `127.0.0.0/8` address — not just
    /// `127.0.0.1`. Anything else, including an unparseable string, is
    /// treated as non-loopback (the fail-closed default).
    private var isLoopbackHost: Bool {
        if host.caseInsensitiveCompare("localhost") == .orderedSame {
            return true
        }
        if host == "::1" {
            return true
        }
        let octets = host.split(separator: ".", omittingEmptySubsequences: false)
        guard octets.count == 4 else { return false }
        let parsed = octets.map { UInt8($0) }
        guard parsed.allSatisfy({ $0 != nil }) else { return false }
        return parsed[0] == 127
    }

    /// `true` when the scheme should be upgraded to `https`/`wss` — any
    /// non-loopback host, unless the operator explicitly opted into
    /// cleartext via `allowInsecureHTTP`.
    private var useSecureScheme: Bool {
        !isLoopbackHost && !allowInsecureHTTP
    }

    // Plain http/ws, not https/wss. Fine against the default loopback
    // `host` — App Transport Security's own loopback exemption is what lets
    // this work at all, and ATS should otherwise block a non-loopback
    // `http://` target outright. But if `host` is ever repointed at a real
    // LAN address (e.g. iOS talking to a Mac), the Authorization: Bearer
    // header would travel in cleartext should that ATS assumption not hold
    // — see docs/security/TRIFECTA_PATHS.md §8 (Sprint S12, Phase 4).
    var baseURL: URL? {
        URL(string: "\(useSecureScheme ? "https" : "http")://\(host):\(port)")
    }

    var webSocketURL: URL? {
        URL(string: "\(useSecureScheme ? "wss" : "ws")://\(host):\(port)/ws")
    }

    static let `default` = ServerConfig(host: "127.0.0.1", port: 3000, token: nil)

    // MARK: Persistence

    private static let hostKey = "lopi.server.host"
    private static let portKey = "lopi.server.port"
    private static let allowInsecureHTTPKey = "lopi.server.allowInsecureHTTP"
    private static let keychainAccount = "lopi.server.token"

    static func load() -> ServerConfig {
        let defaults = UserDefaults.standard
        let host = defaults.string(forKey: hostKey) ?? `default`.host
        let port = defaults.object(forKey: portKey) as? Int ?? `default`.port
        let allowInsecureHTTP = defaults.bool(forKey: allowInsecureHTTPKey)
        return ServerConfig(
            host: host,
            port: port,
            token: Keychain.read(keychainAccount),
            allowInsecureHTTP: allowInsecureHTTP
        )
    }

    func save() {
        let defaults = UserDefaults.standard
        defaults.set(host, forKey: Self.hostKey)
        defaults.set(port, forKey: Self.portKey)
        defaults.set(allowInsecureHTTP, forKey: Self.allowInsecureHTTPKey)
        if let token, !token.isEmpty {
            Keychain.write(token, account: Self.keychainAccount)
        } else {
            Keychain.delete(Self.keychainAccount)
        }
    }
}

/// Minimal generic-password Keychain wrapper.
enum Keychain {
    private static let service = "ai.konjo.lopi"

    static func read(_ account: String) -> String? {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ]
        var result: AnyObject?
        guard SecItemCopyMatching(query as CFDictionary, &result) == errSecSuccess,
              let data = result as? Data,
              let value = String(data: data, encoding: .utf8)
        else { return nil }
        return value
    }

    static func write(_ value: String, account: String) {
        delete(account)
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecValueData as String: Data(value.utf8),
        ]
        SecItemAdd(query as CFDictionary, nil)
    }

    static func delete(_ account: String) {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
        SecItemDelete(query as CFDictionary)
    }
}
