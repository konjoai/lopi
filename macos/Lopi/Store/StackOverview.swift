import SwiftUI
import LopiStacksKit

/// Projects `StackStore.panes` + the live `liveAgents` map into the
/// `/overview` board: one card per stack, bucketed into four lifecycle
/// columns (queued/running/testing/done). Swift mirror of web's
/// `stores/stackOverview.ts`. Lives beside `Store/Overview.swift` (not
/// `LopiStacksKit`) for the same reason that file does — it computes `Color`
/// directly and projects live agent state, so it isn't the portable domain
/// layer `StackTypes.swift`/`StackRun.swift` are.

/// The four board columns, in display + `CaseIterable` order.
enum StackLifecycle: String, CaseIterable, Identifiable {
    case queued, running, testing, done

    var id: String { rawValue }

    var label: String {
        switch self {
        case .queued: return "Queued"
        case .running: return "Running"
        case .testing: return "Testing"
        case .done: return "Done"
        }
    }

    /// Fixed column accent — mirrors the same three Konjo hues used app-wide
    /// for these exact lifecycle meanings (ice=running, violet=testing,
    /// jade=done); queued gets the neutral paper-at-half tone.
    var color: Color {
        switch self {
        case .queued: return Konjo.fg.opacity(0.5)
        case .running: return Konjo.ice
        case .testing: return Konjo.violet
        case .done: return Konjo.jade
        }
    }
}

/// One loop's mini-progress-bar segment.
struct StackLoopDot: Identifiable {
    let id: String
    let color: Color
    let pulsing: Bool
}

/// One stack, ready for the board — already resolved against live agent state.
struct StackOverviewCard: Identifiable {
    var id: String { key }
    let key: String
    let title: String
    let lifecycle: StackLifecycle
    /// True when the stack's most recent run ended in a blocked card.
    let failed: Bool
    /// Left-accent / dot color — the lifecycle color, overridden to rose when `failed`.
    let accentColor: Color
    let loopCount: Int
    let loops: [StackLoopDot]
    let goal: String
    let repo: String
    let branch: String
    /// Right-aligned meta text — elapsed+cost while live, cost/failed once done, "queued" otherwise.
    let metaRight: String
    let metaRightColor: Color
}

/// First non-empty string among `values`, `nil` if every one is missing or
/// empty — the Swift equivalent of JS's `a || b || c` chain over strings.
private func firstNonEmpty(_ values: String?...) -> String? {
    for v in values where v?.isEmpty == false { return v }
    return nil
}

private func repoBasename(_ path: String?) -> String {
    guard let path, !path.isEmpty else { return "" }
    return path.split(separator: "/").last.map(String.init) ?? path
}

private func agentFor(_ card: StackCard, _ agents: [String: LiveAgent]) -> LiveAgent? {
    card.taskId.flatMap { agents[$0] }
}

private func loopDot(_ card: StackCard, accentColor: Color) -> StackLoopDot {
    switch card.status {
    case .done: return StackLoopDot(id: card.id, color: Konjo.jade, pulsing: false)
    case .blocked: return StackLoopDot(id: card.id, color: Konjo.rose, pulsing: false)
    case .running: return StackLoopDot(id: card.id, color: accentColor, pulsing: true)
    default: return StackLoopDot(id: card.id, color: Konjo.fg.opacity(0.15), pulsing: false)
    }
}

/// Resolve one pane's lifecycle bucket + the "representative" card whose
/// goal/repo/branch/agent stand in for the whole stack: the currently
/// running card while live, the most recently executed card once done, or
/// the next-to-run card while queued.
private func classify(_ order: [StackCard], _ agents: [String: LiveAgent]) -> (lifecycle: StackLifecycle, rep: StackCard) {
    if let running = order.first(where: { $0.status == .running }) {
        let testing = agentFor(running, agents)?.phase.lowercased() == "testing"
        return (testing ? .testing : .running, running)
    }
    if order.allSatisfy({ $0.status == .done || $0.status == .blocked }) {
        return (.done, order[order.count - 1])
    }
    let next = order.first { $0.status != .done && $0.status != .blocked }
    return (.queued, next ?? order[0])
}

private func metaFor(
    _ lifecycle: StackLifecycle, _ rep: StackCard, _ order: [StackCard], _ failed: Bool,
    _ accentColor: Color, _ agents: [String: LiveAgent], _ now: Date
) -> (text: String, color: Color) {
    if lifecycle == .running || lifecycle == .testing {
        let agent = agentFor(rep, agents)
        let elapsedMs = agent.map { max(0, now.timeIntervalSince($0.startedAt) * 1000) } ?? 0
        let cost = agent?.costUsd ?? 0
        return ("\(formatElapsed(elapsedMs)) · $\(String(format: "%.4f", cost))", accentColor)
    }
    if lifecycle == .done {
        if failed { return ("failed", Konjo.rose) }
        let total = order.reduce(0.0) { $0 + (agentFor($1, agents)?.costUsd ?? 0) }
        return (String(format: "$%.4f", total), Konjo.fg.opacity(0.4))
    }
    return ("queued", Konjo.fg.opacity(0.4))
}

/// Project every non-bare pane into one board card. Panes with no cards yet
/// (`paneIsBare`) are left off the board — they're an unstarted composer, not
/// a stack worth showing on a lifecycle board. `now` is injectable for
/// deterministic tests — defaults to the real clock for live callers.
func buildStackOverviewCards(_ panes: [StackPaneState], _ agents: [String: LiveAgent], now: Date = Date()) -> [StackOverviewCard] {
    var out: [StackOverviewCard] = []
    for pane in panes {
        if paneIsBare(pane) { continue }
        let order = executionOrder(pane.cards)
        guard !order.isEmpty else { continue }

        let (lifecycle, rep) = classify(order, agents)
        let failed = lifecycle == .done && order.contains { $0.status == .blocked }
        let accentColor = failed ? Konjo.rose : lifecycle.color
        let loops = order.map { loopDot($0, accentColor: accentColor) }
        let meta = metaFor(lifecycle, rep, order, failed, accentColor, agents, now)

        let repo = repoBasename(firstNonEmpty(rep.config.repo, pane.config.defaults.repo))
        let branch = firstNonEmpty(rep.config.branch, pane.config.defaults.branch) ?? SEED_BRANCH

        out.append(StackOverviewCard(
            key: pane.key, title: pane.title, lifecycle: lifecycle, failed: failed,
            accentColor: accentColor, loopCount: order.count, loops: loops,
            goal: rep.goal, repo: repo.isEmpty ? "auto" : repo, branch: branch,
            metaRight: meta.text, metaRightColor: meta.color))
    }
    return out
}

/// Group already-built cards by column, in display order.
func groupByLifecycle(_ cards: [StackOverviewCard]) -> [StackLifecycle: [StackOverviewCard]] {
    var groups: [StackLifecycle: [StackOverviewCard]] = [.queued: [], .running: [], .testing: [], .done: []]
    for card in cards { groups[card.lifecycle, default: []].append(card) }
    return groups
}

/// Total cost across every live/historic agent in the map — the board's
/// "spent" stat. Every task originates from a stack card (the app's one
/// create-task entry point), so the whole-map sum is the whole-board spend.
func totalCost(_ agents: [String: LiveAgent]) -> Double {
    agents.values.reduce(0) { $0 + $1.costUsd }
}
