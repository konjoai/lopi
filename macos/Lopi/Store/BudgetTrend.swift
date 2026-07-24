import Foundation

// Pure spend-trend formatting/analysis for the Budget page's 7-day chart —
// split out from `BudgetView` so it's unit-testable without a live view.
// Mirrors web's `budget/+page.svelte` trend computations exactly.

/// One bar-chart-ready trend entry.
struct TrendBar: Equatable {
    let heightPct: Double
    let isToday: Bool
    let label: String
}

/// Lowercase 3-letter weekday abbreviation for a `yyyy-MM-dd` UTC date string.
/// Empty string if `dateStr` doesn't parse.
func weekdayAbbrev(_ dateStr: String) -> String {
    let parser = DateFormatter()
    parser.dateFormat = "yyyy-MM-dd"
    parser.timeZone = TimeZone(identifier: "UTC")
    parser.locale = Locale(identifier: "en_US_POSIX")
    guard let date = parser.date(from: dateStr) else { return "" }
    let formatter = DateFormatter()
    formatter.dateFormat = "EEE"
    formatter.timeZone = TimeZone(identifier: "UTC")
    formatter.locale = Locale(identifier: "en_US_POSIX")
    return formatter.string(from: date).lowercased()
}

/// Bar-chart-ready rows for the 7-day trend — the last entry is always
/// "today", every other label is its weekday abbreviation.
func trendBars(_ trend: [BudgetBreakdown.DaySpend]) -> [TrendBar] {
    let maxCost = max(1, trend.map(\.costUsd).max() ?? 0)
    return trend.enumerated().map { i, day in
        TrendBar(
            heightPct: day.costUsd / maxCost * 100,
            isToday: i == trend.count - 1,
            label: i == trend.count - 1 ? "today" : weekdayAbbrev(day.date))
    }
}

/// Today's spend vs. the average of the prior 6 days. `nil` when there's
/// nothing to compare against, or there's no prior spend and none today
/// either. `pct` is `nil` specifically when prior spend was zero but today
/// has spend — "new spend" can't be expressed as a percentage of zero.
/// Mirrors web's `trendDelta`.
func trendDelta(_ trend: [BudgetBreakdown.DaySpend]) -> (pct: Int?, up: Bool)? {
    guard trend.count >= 2 else { return nil }
    let today = trend[trend.count - 1].costUsd
    let prior = trend.dropLast()
    let priorAvg = prior.reduce(0.0) { $0 + $1.costUsd } / Double(prior.count)
    if priorAvg == 0 {
        return today > 0 ? (nil, true) : nil
    }
    let pct = (today - priorAvg) / priorAvg * 100
    return (Int(abs(pct).rounded()), pct >= 0)
}
