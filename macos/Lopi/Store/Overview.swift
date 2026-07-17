import SwiftUI

// Pure projection from the live session map to the Overview screen's rows —
// the Swift mirror of web's `stores/overview.ts`. Kept free of `AppModel`
// coupling (takes a plain `[LiveAgent]`) so it's unit-testable against a
// seeded array, exactly like the web original is unit-testable against a
// seeded `Map`.

/// One row of the Overview table.
struct OverviewRow: Identifiable, Equatable {
    let id: String
    let goal: String
    /// Always "—" today — `repo` is unwired end-to-end (no backend field on
    /// `TaskStarted`/`GET /api/tasks`), the same real gap web's own Overview
    /// has. Not a bug to fix as part of this port; see `AppModel+Live.swift`.
    let repo: String
    let branch: String
    let phase: String
    let bucket: FleetBucket
    let elapsedMs: Double
    let cost: Double
    let score: Double?
    let attempt: Int
    let orbColor: Color
    let awaiting: Bool
}

/// The five lifecycle filter chips — `deadLetter` folds failed + cancelled
/// into one chip, matching web's 5-chip layout even though `FleetBucket`
/// itself keeps them as separate buckets.
enum OverviewFilter: String, CaseIterable, Identifiable {
    case all, running, queued, done, deadLetter

    var id: String { rawValue }

    var label: String {
        switch self {
        case .all: return "all"
        case .running: return "running"
        case .queued: return "queued"
        case .done: return "done"
        case .deadLetter: return "dead-letter"
        }
    }
}

/// Sort rank — running first, then queued, then terminal states. Mirrors
/// web's `statusRank`.
private func statusRank(_ bucket: FleetBucket) -> Int {
    switch bucket {
    case .running: return 0
    case .queued: return 1
    case .succeeded: return 2
    case .failed: return 3
    case .cancelled: return 4
    }
}

/// Project the live session map into sorted Overview rows: primary sort by
/// lifecycle rank (running < queued < done < failed < cancelled), secondary
/// by most-recently-started first within a rank. `now` is injectable for
/// deterministic tests — defaults to the real clock for live callers.
func overviewRows(_ agents: [LiveAgent], now: Date = Date()) -> [OverviewRow] {
    let rows = agents.map { a -> OverviewRow in
        OverviewRow(
            id: a.id,
            goal: a.goal,
            repo: "—",
            branch: a.branch ?? "",
            phase: a.phase,
            bucket: FleetBucket.of(a.phase),
            elapsedMs: max(0, now.timeIntervalSince(a.startedAt) * 1000),
            cost: a.costUsd,
            score: a.score,
            attempt: a.attempt,
            orbColor: a.accent,
            awaiting: a.awaitingApproval)
    }
    return rows.sorted { l, r in
        let lr = statusRank(l.bucket), rr = statusRank(r.bucket)
        return lr != rr ? lr < rr : l.elapsedMs > r.elapsedMs
    }
}

func rowMatchesFilter(_ row: OverviewRow, _ filter: OverviewFilter) -> Bool {
    switch filter {
    case .all: return true
    case .running: return row.bucket == .running
    case .queued: return row.bucket == .queued
    case .done: return row.bucket == .succeeded
    case .deadLetter: return row.bucket == .failed || row.bucket == .cancelled
    }
}

func filterRows(_ rows: [OverviewRow], _ filter: OverviewFilter) -> [OverviewRow] {
    filter == .all ? rows : rows.filter { rowMatchesFilter($0, filter) }
}

func filterCounts(_ rows: [OverviewRow]) -> [OverviewFilter: Int] {
    var counts: [OverviewFilter: Int] = [.all: rows.count, .running: 0, .queued: 0, .done: 0, .deadLetter: 0]
    for r in rows {
        switch r.bucket {
        case .running: counts[.running, default: 0] += 1
        case .queued: counts[.queued, default: 0] += 1
        case .succeeded: counts[.done, default: 0] += 1
        case .failed, .cancelled: counts[.deadLetter, default: 0] += 1
        }
    }
    return counts
}

/// Under a minute shows `"{s}s"`; a minute or more shows `"{m}m {s%60}s"` —
/// no hour unit, matching web's `formatElapsed` exactly (a 90-minute task
/// reads "90m 12s", not "1h 30m 12s").
func formatElapsed(_ ms: Double) -> String {
    let s = max(0, Int(ms / 1000))
    let m = s / 60
    return m > 0 ? "\(m)m \(s % 60)s" : "\(s)s"
}

/// Score tier color — mirrors `+page.svelte`'s `scoreColor`.
func overviewScoreColor(_ score: Double) -> Color {
    if score >= 0.8 { return Konjo.jade }
    if score >= 0.5 { return Konjo.sun }
    return Konjo.rose
}
