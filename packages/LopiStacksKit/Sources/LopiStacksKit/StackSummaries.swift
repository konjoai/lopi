import Foundation

// Read-only summary lines + stack-level active-state predicates — the pure port
// of the "hide-inactive" summary block and the `stack*Active`/`stack*Summary`
// family in `stores/stack.ts`. Foundation only.

// MARK: - Per-loop summaries

/// The schedule line shown when `card.scheduled`.
public func scheduleSummary(_ card: StackCard) -> String { cronHuman(card.cron) }

/// The guardrails line shown when `gate || until`.
public func guardSummary(_ card: StackCard) -> String {
    let g = card.guardrails
    var parts: [String] = []
    if g.gate { parts.append("gate") }
    if g.until { parts.append("until") }
    parts.append("budget:\(g.budget.rawValue)")
    parts.append("max \(cardIterationsLabel(card.maxIterations))")
    return parts.joined(separator: " · ")
}

/// The evals line shown when more than the baseline is on.
public func evalsSummary(_ card: StackCard) -> String {
    let n = card.evals.count
    if n <= 1 { return "1 check · baseline only" }
    return "\(n) checks · baseline + \(n - 1) more"
}

// MARK: - Stack-level active-state predicates

/// A chain guardrails facet reads "active" once `onFail` moved off the
/// do-nothing default (`.stop`).
public func stackGuardActive(_ g: StackGuardrails) -> Bool { g.onFail != .stop }

public func stackEvalActive(_ config: StackConfig) -> Bool { config.evals.count > 1 }

/// B1 — the goal facet reads "active" once run-until-goal is switched on.
public func stackGoalActive(_ config: StackConfig) -> Bool { config.goal.pursue }

/// True only when run-until-goal is on *and* there is a real acceptance to
/// pursue — the exact condition `runStack` gates chain re-running on.
public func stackPursuesGoal(_ config: StackConfig) -> Bool {
    config.goal.pursue && stackEvalActive(config)
}

/// The goal summary line for the dock.
public func stackGoalSummary(_ config: StackConfig) -> String {
    let ceiling = config.loopCount == 0 ? "until met" : "≤\(config.loopCount) chain-runs"
    return "pursue chain acceptance · \(ceiling)"
}

/// The stack's own defaults read "active" once any field has moved off the
/// app-wide baseline.
public func stackDefaultsActive(_ defaults: StackDefaults) -> Bool {
    defaults.model != DEFAULT_STACK_DEFAULTS.model
        || defaults.effort != DEFAULT_STACK_DEFAULTS.effort
        || defaults.repo != DEFAULT_STACK_DEFAULTS.repo
        || defaults.branch != DEFAULT_STACK_DEFAULTS.branch
        || defaults.autonomy != DEFAULT_STACK_DEFAULTS.autonomy
}

// MARK: - Stack-level summaries

public func stackGuardSummary(_ g: StackGuardrails) -> String {
    "\(g.onFail.rawValue) · budget:\(g.budget.rawValue)"
}

public func stackEvalsSummary(_ config: StackConfig) -> String {
    let n = config.evals.count
    if n <= 1 { return "1 check · baseline only" }
    return "\(n) checks · chain acceptance"
}

/// The stack defaults summary line: which model (and, when set, repo) every
/// loop inherits, per the mockup's "default model X · every loop inherits"
/// copy. Uses the option's display label rather than the raw wire value —
/// load-bearing for `auto`, whose raw value would otherwise render the bare
/// sentinel string instead of a real display string. `repoLabel` is the
/// caller's already-resolved display label for `defaults.repo` (see
/// `repoLabelForPath`) — this function stays repo-catalog-agnostic, same as
/// every other summary helper in this file. Omitted from the summary
/// entirely when no repo override is set (`defaults.repo == ""`). Mirrors
/// the web `stackDefaultsSummary`.
public func stackDefaultsSummary(_ defaults: StackDefaults, repoLabel: String? = nil) -> String {
    let modelLabel = MODEL_OPTIONS.first { $0.value == defaults.model }?.label ?? defaults.model
    let repoPart = (!defaults.repo.isEmpty && repoLabel != nil) ? " · repo \(repoLabel!)" : ""
    return "model \(modelLabel)\(repoPart) · every loop inherits"
}

/// While the stack drives cadence (own schedule, or chain-loop > 1), a card's
/// own `scheduled` flag must not be presented as independently active. Purely a
/// rendering rule — never mutates the card's stored cron.
public func perLoopScheduleGoverned(_ config: StackConfig) -> Bool {
    config.scheduled || config.loopCount != 1
}
