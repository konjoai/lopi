import Foundation

// `formatElapsed` is the one survivor of the pre-kanban per-agent Overview
// rollup (the Swift mirror of web's `stores/overview.ts`) — `OverviewView`
// now renders `StackOverview.swift`'s per-stack board instead, the same
// redesign web shipped (`stores/stackOverview.ts` importing `formatElapsed`
// from `overview.ts` rather than duplicating it). The rest of the old
// projection (`OverviewRow`/`OverviewFilter`/`overviewRows`/`filterRows`/
// `filterCounts`/`overviewScoreColor`) has zero remaining callers post-port
// and was removed rather than left as dead code.

/// Under a minute shows `"{s}s"`; a minute or more shows `"{m}m {s%60}s"` —
/// no hour unit, matching web's `formatElapsed` exactly (a 90-minute task
/// reads "90m 12s", not "1h 30m 12s").
func formatElapsed(_ ms: Double) -> String {
    let s = max(0, Int(ms / 1000))
    let m = s / 60
    return m > 0 ? "\(m)m \(s % 60)s" : "\(s)s"
}
