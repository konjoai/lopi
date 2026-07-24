import Foundation

// Pure by-repo cost grouping for the Budget page's breakdown panel — split
// out of `BudgetView` so it's unit-testable without a live view, the same
// reasoning `BudgetTrend.swift` follows for the spend-trend chart. Mirrors
// web's `stores/budget.ts` `byRepo` derived store: session-scoped, grouped
// from the live agent map (not a server-side query), since `repo` is only
// carried on live wire events and isn't queryable per-task from the DB the
// way `byModel`'s server-side breakdown is.

/// One repo's total live/session cost.
struct RepoSpend: Identifiable, Equatable {
    let name: String
    let cost: Double
    var id: String { name }
}

/// Cost grouped by repo (basenamed for display), from the live session's
/// agent map, sorted highest-spend first. Agents with zero or negative cost
/// are excluded, matching web's `byRepo` filter. A blank/never-started repo
/// groups under `"auto"` — the same fallback label `Store/StackOverview.swift`
/// uses for a card with no resolved repo.
func groupCostByRepo(_ agents: [String: LiveAgent]) -> [RepoSpend] {
    var totals: [String: Double] = [:]
    for agent in agents.values where agent.costUsd > 0 {
        let name = repoBasename(agent.repo)
        totals[name.isEmpty ? "auto" : name, default: 0] += agent.costUsd
    }
    return totals
        .map { RepoSpend(name: $0.key, cost: $0.value) }
        .sorted { $0.cost > $1.cost }
}
