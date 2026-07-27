import XCTest
@testable import Lopi

/// Sprint S11 Round 2 — cleartext HTTP audit fix.
///
/// `ServerConfig.baseURL`/`webSocketURL` must default to `https`/`wss` for
/// any non-loopback host (fail-closed, mirroring `lopi-ui`'s
/// `auth_policy::validate_auth_policy`), while never regressing today's
/// local-dev `http`/`ws` behavior on loopback hosts.
final class ServerConfigTests: XCTestCase {
    func testLoopbackHostWithoutOptInUsesPlaintext() {
        let cfg = ServerConfig(host: "127.0.0.1", port: 3000, token: nil, allowInsecureHTTP: false)
        XCTAssertEqual(cfg.baseURL?.scheme, "http")
        XCTAssertEqual(cfg.webSocketURL?.scheme, "ws")
    }

    func testLoopbackHostWithOptInStillUsesPlaintext() {
        // Opt-in is irrelevant/harmless on loopback — already plaintext.
        let cfg = ServerConfig(host: "127.0.0.1", port: 3000, token: nil, allowInsecureHTTP: true)
        XCTAssertEqual(cfg.baseURL?.scheme, "http")
        XCTAssertEqual(cfg.webSocketURL?.scheme, "ws")
    }

    func testLoopbackAliasesAlsoUsePlaintext() {
        for host in ["localhost", "LOCALHOST", "::1", "127.0.0.2"] {
            let cfg = ServerConfig(host: host, port: 3000, token: nil, allowInsecureHTTP: false)
            XCTAssertEqual(cfg.baseURL?.scheme, "http", "\(host) should be treated as loopback")
            XCTAssertEqual(cfg.webSocketURL?.scheme, "ws", "\(host) should be treated as loopback")
        }
    }

    func testNonLoopbackHostWithoutOptInUsesTLS() {
        let cfg = ServerConfig(host: "example.com", port: 3000, token: nil, allowInsecureHTTP: false)
        XCTAssertEqual(cfg.baseURL?.scheme, "https")
        XCTAssertEqual(cfg.webSocketURL?.scheme, "wss")
    }

    func testNonLoopbackHostWithOptInUsesPlaintext() {
        let cfg = ServerConfig(host: "example.com", port: 3000, token: nil, allowInsecureHTTP: true)
        XCTAssertEqual(cfg.baseURL?.scheme, "http")
        XCTAssertEqual(cfg.webSocketURL?.scheme, "ws")
    }
}
