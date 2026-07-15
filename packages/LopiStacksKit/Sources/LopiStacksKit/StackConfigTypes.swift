import Foundation

// Stack-level (per-pane) config types — the pure port of the `StackConfig`
// family in `stores/stack.ts` plus `stores/stackDefaults.ts`. Foundation only.

// MARK: - Stack defaults (per-pane baseline for every card's config override)

/// Per-stack default config every loop's `CardConfig` override falls back to.
/// `model`/`effort`/`repo` are real `CreateTaskRequest` fields; `autonomy` is
/// client-only. `branch` has no field of its own but still reaches the server:
/// `StackPayload` turns it into a "Target branch: …" planning constraint.
public struct StackDefaults: Codable, Hashable {
    public var model: String
    public var effort: String
    public var repo: String
    public var branch: String
    public var autonomy: String

    public init(model: String, effort: String, repo: String, branch: String, autonomy: String) {
        self.model = model
        self.effort = effort
        self.repo = repo
        self.branch = branch
        self.autonomy = autonomy
    }
}

/// A selectable option: stable value + human label + optional hint. Mirrors the
/// web `Option` shape (kept local to the pure layer so it needs no UI import).
public struct StackOption: Hashable {
    public var value: String
    public var label: String
    public var hint: String = ""
    /// Section this option belongs to, or `nil` to pin it above every section.
    /// Only the repo catalog groups; every other field leaves this `nil` and so
    /// renders as one flat list — see `OptionMenu.swift`.
    public var group: String? = nil

    public init(value: String, label: String, hint: String = "", group: String? = nil) {
        self.value = value
        self.label = label
        self.hint = hint
        self.group = group
    }
}

/// The real `AutonomyLevel` ladder (`crates/lopi-core/src/loop_config.rs`).
public let AUTONOMY_OPTIONS: [StackOption] = [
    StackOption(value: "L1", label: "L1 · Report only", hint: "report only, no PR"),
    StackOption(value: "L2", label: "L2 · Draft PR", hint: "draft PR, human approves"),
    StackOption(value: "L3", label: "L3 · Verified PR", hint: "verify before PR"),
    StackOption(value: "L4", label: "L4 · Auto-merge", hint: "auto-merge on pass")
]

/// The branch a fresh stack starts on, before any repo has been picked. The
/// live dropdowns no longer read this — they derive their options from
/// `AppModel.branchesByRepo`, fetched from `/api/branches`. This is only the
/// cold-start seed for `DEFAULT_STACK_DEFAULTS`, which lives in this
/// Foundation-only pure layer and so cannot reach the network.
public let SEED_BRANCH = "main"

/// Pick the branch to display for a repo, given that repo's real branches.
///
/// An empty `branches` means we have no knowledge of the repo — unfetched, or
/// the fetch failed — so `current` is returned untouched rather than being
/// second-guessed away. Otherwise an explicit, still-valid choice always wins;
/// only an unset or now-invalid branch falls back to the repo's HEAD. `branch`
/// is not inert: it reaches the server as a planning constraint via
/// `StackPayload`, so showing one branch while storing another would silently
/// launch against the wrong target.
///
/// The 1:1 port of web's `resolveBranch` (`stores/stackDefaults.ts`) — the two
/// surfaces must agree on which branch a repo switch lands on.
public func resolveBranch(_ current: String, _ branches: [String], _ head: String) -> String {
    guard let first = branches.first else { return current }
    if !current.isEmpty, branches.contains(current) { return current }
    return branches.contains(head) ? head : first
}

/// Worker-model options — the same catalog `LaunchControls.models` carries, kept
/// here so the pure layer's `DEFAULT_STACK_DEFAULTS.model` matches the app.
public let MODEL_OPTIONS: [StackOption] = [
    StackOption(value: "claude-opus-4-8", label: "Opus 4.8", hint: "deepest"),
    StackOption(value: "claude-sonnet-5", label: "Sonnet 5", hint: "balanced"),
    StackOption(value: "claude-sonnet-4-6", label: "Sonnet 4.6", hint: "balanced"),
    StackOption(value: "claude-haiku-4-5", label: "Haiku 4.5", hint: "fastest")
]

/// Reasoning-effort options.
public let EFFORT_OPTIONS: [StackOption] = [
    StackOption(value: "low", label: "Low"),
    StackOption(value: "medium", label: "Medium"),
    StackOption(value: "high", label: "High"),
    StackOption(value: "max", label: "Max")
]

/// The app-wide `DEF` a stack's own defaults start from and are compared
/// against (`stackDefaultsActive`).
public let DEFAULT_STACK_DEFAULTS = StackDefaults(
    model: MODEL_OPTIONS[0].value,
    effort: "medium",
    repo: "",
    branch: SEED_BRANCH,
    autonomy: "L2"
)

/// Fresh defaults for a newly-created stack (value type — no shared reference).
public func defaultStackDefaults() -> StackDefaults { DEFAULT_STACK_DEFAULTS }

// MARK: - Chain-scope guardrails / goal

/// The chain-level analogue of a loop's `Guardrails` — deliberately narrower
/// (no gate/until: those are shell pre/exit conditions around a *single* task's
/// retry loop, with nowhere to run at chain scope). `onFail` is WIRED into the
/// chain sequencer; `budget` stays client-only.
public struct StackGuardrails: Codable, Hashable {
    public var onFail: OnFail
    public var budget: Budget

    public init(onFail: OnFail, budget: Budget) {
        self.onFail = onFail
        self.budget = budget
    }
}

/// Freshly-initialized chain guardrails.
public func defaultStackGuardrails() -> StackGuardrails {
    StackGuardrails(onFail: .stop, budget: .auto)
}

/// The stack control area's placement. `dock` is the shipped default (a
/// collapsible strip); `sticky` is the always-expanded variant whose CSS ships
/// unused — flipping this constant is the whole migration (the `SIDEBAR_MODE`
/// precedent). Not user-facing this sprint.
public enum StackControlMode { case dock, sticky }
public let STACK_CONTROL_MODE: StackControlMode = .dock

/// A chain run's default iteration count — `1` (run once through), not the
/// per-loop `DEFAULT_MAX_ITERATIONS`. Reuses the `0` = infinite sentinel.
public let DEFAULT_STACK_LOOP_COUNT = 1

/// B1 — default no-progress tolerance for a goal-pursuing stack.
public let DEFAULT_NO_PROGRESS_LIMIT = 3

/// B1 — the stack's run-until-goal facet. Off by default (additive/backward-
/// compatible).
public struct StackGoal: Codable, Hashable {
    /// Run-until-goal on/off.
    public var pursue: Bool
    /// Consecutive non-gaining chain-runs tolerated before a `no_progress` stop;
    /// `0` disables the no-progress detector.
    public var noProgressLimit: Int

    public init(pursue: Bool, noProgressLimit: Int) {
        self.pursue = pursue
        self.noProgressLimit = noProgressLimit
    }
}

/// Freshly-initialized goal facet.
public func defaultStackGoal() -> StackGoal {
    StackGoal(pursue: false, noProgressLimit: DEFAULT_NO_PROGRESS_LIMIT)
}

// MARK: - Stack config (the purple control area's full state)

/// Stack-level config. `scheduled`/`cron` are STUBBED (no whole-chain cron
/// server-side). `evals` is CLIENT-ONLY chain-acceptance intent. `defaults` is
/// WIRED (resolved into every loop's payload). `goal` is B1 run-until-goal.
public struct StackConfig: Codable, Hashable {
    public var loopCount: Int
    public var scheduled: Bool
    public var cron: CronConfig
    public var guardrails: StackGuardrails
    public var evals: [EvalRef]
    public var defaults: StackDefaults
    public var goal: StackGoal

    public init(loopCount: Int, scheduled: Bool, cron: CronConfig, guardrails: StackGuardrails,
                evals: [EvalRef], defaults: StackDefaults, goal: StackGoal) {
        self.loopCount = loopCount
        self.scheduled = scheduled
        self.cron = cron
        self.guardrails = guardrails
        self.evals = evals
        self.defaults = defaults
        self.goal = goal
    }
}

/// Freshly-initialized stack config — every pane gets its own value.
public func defaultStackConfig() -> StackConfig {
    StackConfig(
        loopCount: DEFAULT_STACK_LOOP_COUNT,
        scheduled: false,
        cron: defaultCron(),
        guardrails: defaultStackGuardrails(),
        evals: [BASELINE_EVAL],
        defaults: defaultStackDefaults(),
        goal: defaultStackGoal()
    )
}

// MARK: - Pane state

/// One independent stack pane — `key` is its stable identity for keyed ops.
/// `draft` is the pane's live composer-replacement card (Creation-Flow-1),
/// pinned above `cards` and never a member of it. The custom init defaults it to
/// a fresh draft so every existing construction site stays unchanged.
public struct StackPaneState: Codable, Hashable, Identifiable {
    public var key: String
    public var title: String
    public var cards: [StackCard]
    public var config: StackConfig
    public var draft: StackCard

    public var id: String { key }

    public init(key: String, title: String, cards: [StackCard], config: StackConfig, draft: StackCard = makeDraft()) {
        self.key = key
        self.title = title
        self.cards = cards
        self.config = config
        self.draft = draft
    }
}

// MARK: - Pane-level defaults a card's config falls back to (the 3 WIRED fields)

/// Pane-level defaults a card's `config` overrides fall back to. A superset —
/// `StackDefaults` — is accepted anywhere this is (the production shape).
public struct PaneDefaults {
    public var model: String
    public var effort: String
    public var repo: String

    public init(model: String, effort: String, repo: String) {
        self.model = model
        self.effort = effort
        self.repo = repo
    }
}

public extension PaneDefaults {
    /// Resolve from a full `StackDefaults` — the production call shape, where a
    /// pane's `config.defaults` supplies all three WIRED fields.
    init(_ d: StackDefaults) {
        self.init(model: d.model, effort: d.effort, repo: d.repo)
    }
}
