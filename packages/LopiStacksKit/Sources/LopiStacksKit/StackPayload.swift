import Foundation

// Backend round-trip: the WIRED card/pane fields → the real create-task payload
// shape. Pure port of `stores/stack.ts`'s `evalsToAcceptance` /
// `cardToTaskPayload` / `paneSubmitPayload` family + the run-order/dry-run/bump
// helpers. Foundation only.
//
// `StackTaskOptions` mirrors the web `CreateTaskOptions` (a superset of the
// macOS `CreateTaskBody`). The extra WIRED fields (max_iterations/on_fail/
// gate/until/acceptance/client_ref/budget_tokens) round-trip correctly here and
// are proven by unit test; the live launch seam (`StackRun`) maps this onto the
// subset the backend body accepts today, honestly dropping what the wire type
// doesn't carry yet — the same honesty stance the web layer takes.

// MARK: - Acceptance

/// One acceptance check's spec — the tier→spec routing A1 defines.
public enum AcceptanceSpec: Equatable {
    case executionOk
    case judge(rubricName: String, criteria: [String])
    case suite(name: String)

    /// The wire `kind` discriminant, matching the web `spec.kind`.
    public var kind: String {
        switch self {
        case .executionOk: return "execution_ok"
        case .judge: return "judge"
        case .suite: return "suite"
        }
    }
}

/// One acceptance check the backend's tiered eval executor scores against.
public struct AcceptanceCheck: Equatable {
    public var tier: EvalTier
    public var spec: AcceptanceSpec
    public var weight: Int
    public var required: Bool

    public init(tier: EvalTier, spec: AcceptanceSpec, weight: Int, required: Bool) {
        self.tier = tier
        self.spec = spec
        self.weight = weight
        self.required = required
    }
}

/// A compiled acceptance goal.
public struct StackAcceptance: Equatable {
    public var checks: [AcceptanceCheck]

    public init(checks: [AcceptanceCheck]) {
        self.checks = checks
    }
}

/// Compile a card's `evals` checklist into a real `StackAcceptance`. Objective
/// (`base`/`test`) → one deterministic `execution_ok` check; `judge` → one judge
/// check whose criteria are the selected judge evals' names; `suite` → one suite
/// check per selected suite eval. Returns `nil` when there is nothing to check.
public func evalsToAcceptance(_ evals: [EvalRef]) -> StackAcceptance? {
    var checks: [AcceptanceCheck] = []
    let hasDeterministic = evals.contains { $0.tier == .base || $0.tier == .test }
    if hasDeterministic {
        checks.append(AcceptanceCheck(tier: .base, spec: .executionOk, weight: 1, required: true))
    }
    let judgeNames = evals.filter { $0.tier == .judge }.map(\.name)
    if !judgeNames.isEmpty {
        checks.append(AcceptanceCheck(
            tier: .judge,
            spec: .judge(rubricName: "ui-evals", criteria: judgeNames),
            weight: 1, required: true))
    }
    for suite in evals.filter({ $0.tier == .suite }) {
        checks.append(AcceptanceCheck(tier: .suite, spec: .suite(name: suite.name), weight: 1, required: true))
    }
    return checks.isEmpty ? nil : StackAcceptance(checks: checks)
}

// MARK: - Task options / payload

/// The create-task options a card/pane would submit as — mirrors the web
/// `CreateTaskOptions`. `nil` means the field is omitted from the payload.
public struct StackTaskOptions: Equatable {
    public var model: String?
    public var effort: String?
    public var maxIterations: Int?
    public var onFail: OnFail?
    public var clientRef: String?
    public var gate: String?
    public var until: String?
    public var budgetTokens: Int?
    public var acceptance: StackAcceptance?
    public var constraints: [String]?

    public init(model: String? = nil, effort: String? = nil, maxIterations: Int? = nil,
                onFail: OnFail? = nil, clientRef: String? = nil, gate: String? = nil,
                until: String? = nil, budgetTokens: Int? = nil, acceptance: StackAcceptance? = nil,
                constraints: [String]? = nil) {
        self.model = model
        self.effort = effort
        self.maxIterations = maxIterations
        self.onFail = onFail
        self.clientRef = clientRef
        self.gate = gate
        self.until = until
        self.budgetTokens = budgetTokens
        self.acceptance = acceptance
        self.constraints = constraints
    }
}

/// A full create-task payload (goal/repo/priority + options).
public struct StackTaskPayload: Equatable {
    public var goal: String
    public var repo: String
    public var priority: String
    public var options: StackTaskOptions

    public init(goal: String, repo: String, priority: String, options: StackTaskOptions) {
        self.goal = goal
        self.repo = repo
        self.priority = priority
        self.options = options
    }
}

/// The payload a card would submit as, resolving `config` overrides against
/// pane defaults. Pure and total — the WIRED-fields round-trip contract.
public func cardToTaskPayload(_ card: StackCard, _ defaults: PaneDefaults) -> StackTaskPayload {
    var options = StackTaskOptions()
    options.model = card.config.model ?? defaults.model
    options.effort = card.config.effort ?? defaults.effort
    // `0` = "off" on the card pill → a single pass on the wire (never the
    // backend's `0` = infinite sentinel). Any positive N passes through.
    options.maxIterations = card.maxIterations == 0 ? 1 : card.maxIterations
    options.onFail = card.guardrails.onFail
    options.clientRef = card.id
    if card.guardrails.gate { options.gate = card.guardrails.gateCmd }
    if card.guardrails.until { options.until = card.guardrails.untilCmd }
    if let budgetTokens = budgetToTokens(card.guardrails.budget) { options.budgetTokens = budgetTokens }
    if let acceptance = evalsToAcceptance(card.evals) { options.acceptance = acceptance }
    return StackTaskPayload(
        goal: card.goal,
        repo: card.config.repo ?? defaults.repo,
        priority: "normal",
        options: options)
}

/// "Run once": identical resolution, but `maxIterations` forced to `1` in the
/// outgoing payload only (never mutating the card's own stored value).
public func cardToTaskPayloadForRunOnce(_ card: StackCard, _ defaults: PaneDefaults) -> StackTaskPayload {
    var payload = cardToTaskPayload(card, defaults)
    payload.options.maxIterations = 1
    return payload
}

/// A bare-prompt launch from a Forge-style pane composer.
public struct PaneLaunch {
    public var goal: String
    public var repo: String
    public var priority: String?
    public var model: String?
    public var effort: String?
    public var branch: String?

    public init(goal: String, repo: String, priority: String? = nil,
         model: String? = nil, effort: String? = nil, branch: String? = nil) {
        self.goal = goal
        self.repo = repo
        self.priority = priority
        self.model = model
        self.effort = effort
        self.branch = branch
    }
}

/// The payload a bare pane prompt submits as — deliberately bare (no stack-loop
/// semantics forced). A branch override surfaces as a planning constraint.
public func paneSubmitPayload(_ launch: PaneLaunch) -> StackTaskPayload {
    var options = StackTaskOptions()
    if let model = launch.model { options.model = model }
    if let effort = launch.effort { options.effort = effort }
    if let branch = launch.branch?.trimmingCharacters(in: .whitespacesAndNewlines), !branch.isEmpty {
        options.constraints = ["Target branch: \(branch)"]
    }
    return StackTaskPayload(
        goal: launch.goal,
        repo: launch.repo,
        priority: (launch.priority?.isEmpty == false ? launch.priority : nil) ?? "normal",
        options: options)
}

// MARK: - Run order + dry run

/// Execution order: bottom-of-stack (oldest, next to run) first, top last —
/// the reverse of array order, since the composer prepends new cards. A draft
/// card is never in `pane.cards`, but any run-plan path must still refuse to
/// schedule one (Creation-Flow-1 §1 — never let `.draft` fall through to a run
/// path), so it is filtered here defensively. Mirrors the web `executionOrder`.
public func executionOrder(_ cards: [StackCard]) -> [StackCard] {
    cards.filter { $0.status != .draft }.reversed()
}

/// One problem `dryRunStack` found with a specific card.
public struct DryRunIssue: Equatable {
    public var cardId: String
    public var message: String

    public init(cardId: String, message: String) {
        self.cardId = cardId
        self.message = message
    }
}

/// One card's resolved plan entry, exactly as `dryRunStack` would submit it.
public struct DryRunPlanEntry: Equatable {
    public var cardId: String
    public var goal: String
    public var repo: String
    public var maxIterations: Int

    public init(cardId: String, goal: String, repo: String, maxIterations: Int) {
        self.cardId = cardId
        self.goal = goal
        self.repo = repo
        self.maxIterations = maxIterations
    }
}

/// The plan-validation result `dryRunStack` returns.
public struct DryRunResult: Equatable {
    public var valid: Bool
    public var issues: [DryRunIssue]
    public var plan: [DryRunPlanEntry]

    public init(valid: Bool, issues: [DryRunIssue], plan: [DryRunPlanEntry]) {
        self.valid = valid
        self.issues = issues
        self.plan = plan
    }
}

/// Validate a pane's execution plan without running anything. Pure and total;
/// never launches.
public func dryRunStack(_ cards: [StackCard], _ defaults: PaneDefaults) -> DryRunResult {
    var issues: [DryRunIssue] = []
    let plan: [DryRunPlanEntry] = executionOrder(cards).map { card in
        let payload = cardToTaskPayload(card, defaults)
        if payload.goal.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            issues.append(DryRunIssue(cardId: card.id, message: "goal is empty"))
        }
        if card.guardrails.gate && card.guardrails.gateCmd.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            issues.append(DryRunIssue(cardId: card.id, message: "gate is enabled with an empty command"))
        }
        if card.guardrails.until && card.guardrails.untilCmd.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            issues.append(DryRunIssue(cardId: card.id, message: "until is enabled with an empty command"))
        }
        return DryRunPlanEntry(
            cardId: card.id,
            goal: payload.goal,
            repo: payload.repo,
            maxIterations: payload.options.maxIterations ?? DEFAULT_MAX_ITERATIONS)
    }
    return DryRunResult(valid: issues.isEmpty, issues: issues, plan: plan)
}

// MARK: - Bump (reorder a queued card within an active run's remaining order)

/// The result of `bumpInOrder` — the swapped order, or a clear rejection.
public enum BumpResult: Equatable {
    case ok([String])
    case err(String)
}

/// Attempt to bump (swap with its immediate neighbor) a not-yet-started card
/// within an active run's remaining execution order. `cursor` and everything at
/// or before it are off-limits. Pure.
public func bumpInOrder(_ order: [String], _ cursor: Int, _ cardId: String, _ direction: BumpDirection) -> BumpResult {
    guard let idx = order.firstIndex(of: cardId) else {
        return .err("card is not part of this run’s plan")
    }
    if idx <= cursor {
        return .err("card is already running or finished — only queued cards can be bumped")
    }
    let targetIdx = direction == .up ? idx - 1 : idx + 1
    if targetIdx <= cursor {
        return .err("cannot bump above the currently running card")
    }
    if targetIdx >= order.count {
        return .err("cannot bump past the end of the queue")
    }
    var next = order
    next.swapAt(idx, targetIdx)
    return .ok(next)
}

/// Bump direction — up (earlier in the queue) or down (later).
public enum BumpDirection: Equatable { case up, down }
