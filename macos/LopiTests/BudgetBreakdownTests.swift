import XCTest
@testable import Lopi

/// `BudgetBreakdown` — decode test for `GET /api/budget/breakdown`'s real
/// JSON shape (`crates/lopi-ui/src/web/budget_handlers.rs`), the pure trend
/// formatting/analysis in `BudgetTrend.swift`, and `groupCostByRepo`
/// (`BudgetRepoBreakdown.swift`) — together the Swift mirror of web's
/// `budget/+page.svelte` + `stores/budget.ts` computations.
final class BudgetBreakdownTests: XCTestCase {
    private let decoder = JSONDecoder()

    func testDecodesRealServerShape() throws {
        let json = """
        {
          "by_model": [
            {"model": "opus", "cost_usd": 1.2345},
            {"model": "sonnet", "cost_usd": 0.5}
          ],
          "trend": [
            {"date": "2026-07-17", "cost_usd": 0.0},
            {"date": "2026-07-23", "cost_usd": 2.5}
          ]
        }
        """.data(using: .utf8)!
        let breakdown = try decoder.decode(BudgetBreakdown.self, from: json)
        XCTAssertEqual(breakdown.byModel.map(\.model), ["opus", "sonnet"])
        XCTAssertEqual(breakdown.byModel[0].costUsd, 1.2345, accuracy: 1e-9)
        XCTAssertEqual(breakdown.trend.map(\.date), ["2026-07-17", "2026-07-23"])
        XCTAssertEqual(breakdown.trend[1].costUsd, 2.5, accuracy: 1e-9)
    }

    func testMissingKeysDefaultToEmptyNotDecodeFailure() throws {
        let breakdown = try decoder.decode(BudgetBreakdown.self, from: "{}".data(using: .utf8)!)
        XCTAssertTrue(breakdown.byModel.isEmpty)
        XCTAssertTrue(breakdown.trend.isEmpty)
    }

    // MARK: weekdayAbbrev

    func testWeekdayAbbrevParsesUTCDate() {
        // 2026-07-20 is a Monday.
        XCTAssertEqual(weekdayAbbrev("2026-07-20"), "mon")
    }

    func testWeekdayAbbrevEmptyForUnparseable() {
        XCTAssertEqual(weekdayAbbrev("not-a-date"), "")
    }

    // MARK: trendBars

    private func day(_ date: String, _ cost: Double) -> BudgetBreakdown.DaySpend {
        BudgetBreakdown.DaySpend(date: date, costUsd: cost)
    }

    func testTrendBarsLastEntryIsAlwaysToday() {
        let bars = trendBars([day("2026-07-22", 1.0), day("2026-07-23", 2.0)])
        XCTAssertEqual(bars.count, 2)
        XCTAssertFalse(bars[0].isToday)
        XCTAssertTrue(bars[1].isToday)
        XCTAssertEqual(bars[1].label, "today")
        XCTAssertEqual(bars[1].heightPct, 100, accuracy: 1e-9, "the max-cost day fills the bar")
        XCTAssertEqual(bars[0].heightPct, 50, accuracy: 1e-9)
    }

    func testTrendBarsEmptyTrendNeverDividesByZero() {
        XCTAssertEqual(trendBars([]), [])
    }

    // MARK: trendDelta

    func testTrendDeltaNilForFewerThanTwoDays() {
        XCTAssertNil(trendDelta([day("2026-07-23", 1.0)]))
    }

    func testTrendDeltaUpWhenTodayExceedsPriorAverage() {
        let trend = [day("2026-07-20", 1.0), day("2026-07-21", 1.0), day("2026-07-22", 1.0), day("2026-07-23", 2.0)]
        let delta = trendDelta(trend)
        XCTAssertEqual(delta?.up, true)
        XCTAssertEqual(delta?.pct, 100, "today (2) is 100% above the prior 3-day average (1)")
    }

    func testTrendDeltaDownWhenTodayIsBelowPriorAverage() {
        let trend = [day("2026-07-22", 2.0), day("2026-07-23", 1.0)]
        let delta = trendDelta(trend)
        XCTAssertEqual(delta?.up, false)
        XCTAssertEqual(delta?.pct, 50)
    }

    func testTrendDeltaNewSpendWhenPriorAverageIsZero() {
        let trend = [day("2026-07-22", 0.0), day("2026-07-23", 3.0)]
        let delta = trendDelta(trend)
        XCTAssertEqual(delta?.up, true)
        XCTAssertNil(delta?.pct, "can't express 'new spend' as a percentage of zero prior spend")
    }

    func testTrendDeltaNilWhenNoSpendAtAll() {
        XCTAssertNil(trendDelta([day("2026-07-22", 0.0), day("2026-07-23", 0.0)]))
    }

    // MARK: groupCostByRepo

    private func agent(_ id: String, repo: String?, cost: Double) -> LiveAgent {
        var a = LiveAgent(id: id, goal: id, phase: "running", attempt: 0)
        a.repo = repo
        a.costUsd = cost
        return a
    }

    func testGroupCostByRepoBasenamesAndSortsBySpend() {
        let agents = [
            "a": agent("a", repo: "/Users/dev/lopi", cost: 0.5),
            "b": agent("b", repo: "/Users/dev/other-repo", cost: 1.5),
            "c": agent("c", repo: "/Users/dev/lopi", cost: 0.25),
        ]
        let grouped = groupCostByRepo(agents)
        XCTAssertEqual(grouped.map(\.name), ["other-repo", "lopi"], "highest spend first")
        XCTAssertEqual(grouped[0].cost, 1.5, accuracy: 1e-9)
        XCTAssertEqual(grouped[1].cost, 0.75, accuracy: 1e-9, "same-repo agents sum together")
    }

    func testGroupCostByRepoExcludesZeroAndNegativeSpend() {
        let agents = [
            "a": agent("a", repo: "/repo/a", cost: 0),
            "b": agent("b", repo: "/repo/b", cost: -1),
        ]
        XCTAssertEqual(groupCostByRepo(agents), [])
    }

    func testGroupCostByRepoFallsBackToAutoWhenRepoIsNilOrEmpty() {
        let agents = [
            "a": agent("a", repo: nil, cost: 0.1),
            "b": agent("b", repo: "", cost: 0.2),
        ]
        let grouped = groupCostByRepo(agents)
        XCTAssertEqual(grouped.count, 1, "nil and empty repo group under the same 'auto' bucket")
        XCTAssertEqual(grouped[0].name, "auto")
        XCTAssertEqual(grouped[0].cost, 0.3, accuracy: 1e-9)
    }
}
