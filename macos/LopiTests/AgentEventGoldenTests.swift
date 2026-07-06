import XCTest
@testable import Lopi

/// Golden-fixture decode test (G3, Swift side).
///
/// Decodes the SAME `crates/lopi-core/tests/fixtures/agent_event_golden.json`
/// that the Rust test (`agent_event_golden.rs`) and the TypeScript parser test
/// decode. All three must agree on field values so a new `AgentEvent` variant
/// cannot drift between the Rust enum, the web parser, and this client.
final class AgentEventGoldenTests: XCTestCase {
    private let taskID = "11111111-1111-4111-8111-111111111111"

    /// The canonical fixture in the repo — resolved from this source file's
    /// path so there is a single source of truth (no copied fixture).
    private func goldenData() throws -> [Data] {
        let repoRoot = URL(fileURLWithPath: #file)
            .deletingLastPathComponent() // LopiTests/
            .deletingLastPathComponent() // macos/
            .deletingLastPathComponent() // repo root
        let url = repoRoot
            .appendingPathComponent("crates/lopi-core/tests/fixtures/agent_event_golden.json")
        let raw = try Data(contentsOf: url)
        let arr = try XCTUnwrap(JSONSerialization.jsonObject(with: raw) as? [[String: Any]])
        return try arr.map { try JSONSerialization.data(withJSONObject: $0) }
    }

    func testGoldenAgentEventsDecodeWithExpectedFields() throws {
        let lines = try goldenData()
        XCTAssertEqual(lines.count, 6, "golden fixture covers all six new variants")
        let events = lines.compactMap { AgentEvent.decode(from: $0) }
        XCTAssertEqual(events.count, 6, "every golden event decodes")

        guard case let .toolCall(id, tool, summary) = events[0] else {
            return XCTFail("event 0 should be toolCall, got \(events[0])")
        }
        XCTAssertEqual(id, taskID)
        XCTAssertEqual(tool, "Bash")
        XCTAssertEqual(summary, "ls -la")

        guard case let .toolResult(_, _, isError, preview) = events[1] else {
            return XCTFail("event 1 should be toolResult, got \(events[1])")
        }
        XCTAssertFalse(isError)
        XCTAssertTrue(preview.contains("notes.txt"))

        guard case let .tokenDelta(_, output, input, cacheRead) = events[2] else {
            return XCTFail("event 2 should be tokenDelta, got \(events[2])")
        }
        XCTAssertEqual(output, 118)
        XCTAssertEqual(input, 3)
        XCTAssertEqual(cacheRead, 16312)

        guard case let .apiRetry(_, status, limitType, util) = events[3] else {
            return XCTFail("event 3 should be apiRetry, got \(events[3])")
        }
        XCTAssertEqual(status, "allowed_warning")
        XCTAssertEqual(limitType, "seven_day")
        XCTAssertEqual(util, 0.92, accuracy: 1e-6)

        guard case let .cost(_, costUsd, turns, sessionId) = events[4] else {
            return XCTFail("event 4 should be cost, got \(events[4])")
        }
        XCTAssertEqual(costUsd, 0.0479, accuracy: 1e-9)
        XCTAssertEqual(turns, 3)
        XCTAssertEqual(sessionId, "4fa68a55-05cf-4878-aa2f-d0edaec6b8a6")

        guard case let .phase(_, phase) = events[5] else {
            return XCTFail("event 5 should be phase, got \(events[5])")
        }
        XCTAssertEqual(phase, "review_ready")
    }
}
