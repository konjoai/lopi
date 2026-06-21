import Foundation

/// How often a schedule fires. Lets the editor offer friendly presets while the
/// backend keeps storing a raw 5-field cron expression.
enum CronFrequency: String, CaseIterable, Identifiable {
    case hourly, daily, weekly, monthly, custom
    var id: String { rawValue }
    var label: String { rawValue.capitalized }
}

/// A structured schedule the editor manipulates; renders to / parses from a
/// 5-field cron string (`min hour dom mon dow`). Day-of-week uses names
/// (MON…SUN) so we never trip over the 0/1/7 numbering ambiguity.
struct CronSpec: Equatable {
    var frequency: CronFrequency = .daily
    var minute: Int = 0
    var hour: Int = 9
    var weekday: Int = 1 // 0 = Sunday … 6 = Saturday
    var dayOfMonth: Int = 1
    var custom: String = "0 9 * * *"

    static let weekdayNames = ["Sunday", "Monday", "Tuesday", "Wednesday",
                               "Thursday", "Friday", "Saturday"]
    private static let dowTokens = ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"]

    /// The 5-field cron expression for the current selection.
    var cron: String {
        switch frequency {
        case .hourly:  return "\(minute) * * * *"
        case .daily:   return "\(minute) \(hour) * * *"
        case .weekly:  return "\(minute) \(hour) * * \(Self.dowTokens[clamp(weekday, 0, 6)])"
        case .monthly: return "\(minute) \(hour) \(clamp(dayOfMonth, 1, 31)) * *"
        case .custom:  return custom.trimmingCharacters(in: .whitespaces)
        }
    }

    /// A human description of *this* spec (e.g. "Weekly · Monday at 09:00").
    var summary: String {
        let t = String(format: "%02d:%02d", hour, minute)
        switch frequency {
        case .hourly:  return "Hourly at :\(String(format: "%02d", minute))"
        case .daily:   return "Daily at \(t)"
        case .weekly:  return "Weekly · \(Self.weekdayNames[clamp(weekday, 0, 6)]) at \(t)"
        case .monthly: return "Monthly · day \(dayOfMonth) at \(t)"
        case .custom:  return cron
        }
    }

    /// Best-effort parse of an existing cron string so the editor opens on the
    /// matching preset; anything non-standard falls back to `.custom`.
    static func parse(_ cron: String) -> CronSpec {
        var spec = CronSpec()
        let f = cron.split(separator: " ").map(String.init)
        guard f.count == 5, f[3] == "*" else {
            spec.frequency = .custom; spec.custom = cron; return spec
        }
        let (minF, hourF, domF, dowF) = (f[0], f[1], f[2], f[4])

        if hourF == "*", domF == "*", dowF == "*", let m = Int(minF) {
            spec.frequency = .hourly; spec.minute = m; return spec
        }
        if domF == "*", dowF == "*", let m = Int(minF), let h = Int(hourF) {
            spec.frequency = .daily; spec.minute = m; spec.hour = h; return spec
        }
        if domF == "*", dowF != "*", let m = Int(minF), let h = Int(hourF),
           let wd = dowIndex(dowF) {
            spec.frequency = .weekly; spec.minute = m; spec.hour = h; spec.weekday = wd; return spec
        }
        if dowF == "*", let m = Int(minF), let h = Int(hourF), let d = Int(domF) {
            spec.frequency = .monthly; spec.minute = m; spec.hour = h; spec.dayOfMonth = d; return spec
        }
        spec.frequency = .custom; spec.custom = cron; return spec
    }

    /// Human-readable description for any cron string (used by the list rows).
    static func describe(_ cron: String) -> String { parse(cron).summary }

    private static func dowIndex(_ s: String) -> Int? {
        if let n = Int(s) { return n == 7 ? 0 : (0...6).contains(n) ? n : nil }
        return dowTokens.firstIndex(of: s.uppercased())
    }

    private func clamp(_ v: Int, _ lo: Int, _ hi: Int) -> Int { min(max(v, lo), hi) }
}
