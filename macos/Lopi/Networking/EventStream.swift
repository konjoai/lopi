import Foundation

/// Live connection state, mirroring the web UI's LED indicator.
enum ConnectionState: Equatable {
    case offline
    case connecting
    case live
}

/// Wraps a `URLSessionWebSocketTask` against `/ws`. Yields decoded
/// `AgentEvent`s as an `AsyncStream` and auto-reconnects with backoff. The
/// initial `{type:"snapshot", …}` frame is surfaced separately so callers can
/// seed task + stats state on (re)connect.
actor EventStream {
    private var task: URLSessionWebSocketTask?
    private let session: URLSession = .shared
    private var running = false

    /// Called on the main actor when the connection state changes.
    var onState: (@Sendable (ConnectionState) -> Void)?
    /// Called with the snapshot frame's raw JSON object on connect.
    var onSnapshot: (@Sendable ([String: Any]) -> Void)?
    /// Called for each decoded live event.
    var onEvent: (@Sendable (AgentEvent) -> Void)?

    func setHandlers(
        onState: @escaping @Sendable (ConnectionState) -> Void,
        onSnapshot: @escaping @Sendable ([String: Any]) -> Void,
        onEvent: @escaping @Sendable (AgentEvent) -> Void
    ) {
        self.onState = onState
        self.onSnapshot = onSnapshot
        self.onEvent = onEvent
    }

    /// Start the connect/receive/reconnect loop for `url`.
    func start(url: URL) {
        guard !running else { return }
        running = true
        Task { await loop(url: url) }
    }

    func stop() {
        running = false
        task?.cancel(with: .goingAway, reason: nil)
        task = nil
        onState?(.offline)
    }

    private func loop(url: URL) async {
        var backoff: UInt64 = 1_000_000_000 // 1s, capped at 8s
        while running {
            onState?(.connecting)
            let socket = session.webSocketTask(with: url)
            task = socket
            socket.resume()
            onState?(.live)
            backoff = 1_000_000_000

            // Receive until the socket errors, then fall through to reconnect.
            do {
                while running {
                    let message = try await socket.receive()
                    handle(message)
                }
            } catch {
                // Connection dropped — fall through to backoff + retry.
            }

            guard running else { break }
            onState?(.offline)
            try? await Task.sleep(nanoseconds: backoff)
            backoff = min(backoff * 2, 8_000_000_000)
        }
    }

    private func handle(_ message: URLSessionWebSocketTask.Message) {
        let data: Data
        switch message {
        case let .data(d): data = d
        case let .string(s): data = Data(s.utf8)
        @unknown default: return
        }

        // The first frame is a snapshot envelope; everything else is an event.
        if let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
           obj["type"] as? String == "snapshot" {
            onSnapshot?(obj)
            return
        }
        if let event = AgentEvent.decode(from: data) {
            onEvent?(event)
        }
    }
}
