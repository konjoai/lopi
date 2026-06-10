import Foundation
import Security

/// Connection settings for the lopi server. Host/port live in `UserDefaults`;
/// the Bearer token is stored in the macOS Keychain.
struct ServerConfig: Equatable {
    var host: String
    var port: Int
    var token: String?

    var baseURL: URL? {
        URL(string: "http://\(host):\(port)")
    }

    var webSocketURL: URL? {
        URL(string: "ws://\(host):\(port)/ws")
    }

    static let `default` = ServerConfig(host: "127.0.0.1", port: 3000, token: nil)

    // MARK: Persistence

    private static let hostKey = "lopi.server.host"
    private static let portKey = "lopi.server.port"
    private static let keychainAccount = "lopi.server.token"

    static func load() -> ServerConfig {
        let defaults = UserDefaults.standard
        let host = defaults.string(forKey: hostKey) ?? `default`.host
        let port = defaults.object(forKey: portKey) as? Int ?? `default`.port
        return ServerConfig(host: host, port: port, token: Keychain.read(keychainAccount))
    }

    func save() {
        let defaults = UserDefaults.standard
        defaults.set(host, forKey: Self.hostKey)
        defaults.set(port, forKey: Self.portKey)
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
