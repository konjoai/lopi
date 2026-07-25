import Foundation

/// Formats a server uptime in seconds as a compact human string. Shared by
/// macOS's `DashboardView`/`ConfigView` and iOS's `ServerConfigScreen`.
enum Uptime {
    static func string(_ secs: Int) -> String {
        if secs >= 3600 { return "\(secs / 3600)h \((secs % 3600) / 60)m" }
        if secs >= 60 { return "\(secs / 60)m" }
        return "\(secs)s"
    }
}
