import Foundation

// Cron helpers — the pure port of `stores/stack.ts`'s cron block. Standard
// 5-field expressions only; wildcards, exact numbers, comma lists, and step
// values per field. Foundation only.

private let DOW_TO_NUM: [Dow: Int] = [.Sun: 0, .Mon: 1, .Tue: 2, .Wed: 3, .Thu: 4, .Fri: 5, .Sat: 6]

private func to24Hour(_ hour12: Int, _ ampm: CronConfig.AmPm) -> Int {
    let h = hour12 % 12
    return ampm == .PM ? h + 12 : h
}

/// Derive the standard 5-field cron string from a preset cadence. Returns
/// `c.raw` verbatim when `freq == .custom`.
public func buildCronString(_ c: CronConfig) -> String {
    switch c.freq {
    case .everyMinute:
        return "* * * * *"
    case .hourly:
        return "\(c.min) * * * *"
    case .daily:
        return "\(c.min) \(to24Hour(c.hour12, c.ampm)) * * *"
    case .weekly:
        return "\(c.min) \(to24Hour(c.hour12, c.ampm)) * * \(DOW_TO_NUM[c.dow] ?? 0)"
    case .custom:
        return c.raw
    }
}

private func matchesCronField(_ field: String, _ value: Int) -> Bool {
    if field == "*" { return true }
    return field.split(separator: ",").contains { part in
        if part.hasPrefix("*/"), let n = Int(part.dropFirst(2)), n != 0 {
            return value % n == 0
        }
        return Int(part) == value
    }
}

/// Search forward minute-by-minute from `from` for the next `count` times a
/// standard 5-field cron fires. Bounded to ~40 days so an unsatisfiable
/// expression can't spin forever. Unknown syntax (or a non-5-field string)
/// yields no results rather than throwing.
public func computeNextRuns(_ cronExpr: String, from: Date, count: Int = 3, calendar: Calendar = .current) -> [Date] {
    let fields = cronExpr.split(whereSeparator: { $0.isWhitespace }).map(String.init)
    guard fields.count == 5 else { return [] }
    let (minF, hourF, domF, monF, dowF) = (fields[0], fields[1], fields[2], fields[3], fields[4])

    var results: [Date] = []
    let comps = calendar.dateComponents([.year, .month, .day, .hour, .minute], from: from)
    guard let truncated = calendar.date(from: comps),
          var cursor = calendar.date(byAdding: .minute, value: 1, to: truncated) else { return [] }

    let limitMinutes = 60 * 24 * 40
    var i = 0
    while i < limitMinutes && results.count < count {
        let c = calendar.dateComponents([.month, .day, .hour, .minute, .weekday], from: cursor)
        let weekday0 = (c.weekday ?? 1) - 1 // Calendar: 1=Sun…7=Sat → 0…6
        if matchesCronField(minF, c.minute ?? -1),
           matchesCronField(hourF, c.hour ?? -1),
           matchesCronField(domF, c.day ?? -1),
           matchesCronField(monF, c.month ?? -1),
           matchesCronField(dowF, weekday0) {
            results.append(cursor)
        }
        guard let next = calendar.date(byAdding: .minute, value: 1, to: cursor) else { break }
        cursor = next
        i += 1
    }
    return results
}

/// Human-readable echo of a cron config's cadence.
public func cronHuman(_ c: CronConfig) -> String {
    let mm = String(format: "%02d", c.min)
    switch c.freq {
    case .everyMinute:
        return "every minute"
    case .hourly:
        return "every hour at :\(mm)"
    case .daily:
        return "every day at \(c.hour12):\(mm) \(c.ampm.rawValue)"
    case .weekly:
        return "every \(c.dow.rawValue) at \(c.hour12):\(mm) \(c.ampm.rawValue)"
    case .custom:
        return "custom cron"
    }
}
