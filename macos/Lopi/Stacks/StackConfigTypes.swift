import Foundation

// Stack-level (per-pane) config types — the pure port of the `StackConfig`
// family in `stores/stack.ts` plus `stores/stackDefaults.ts`. Foundation only.

// MARK: - Stack defaults (per-pane baseline for every card's config override)

/// Per-stack default config every loop's `CardConfig` override falls back to.
/// `model`/`effort`/`repo` are real `CreateTaskRequest` fields; `branch`/
/// `autonomy` are client-only.
struct StackDefaults: Codable, Hashable {
    var model: String
    var effort: String
    var repo: String
    var branch: String
    var autonomy: String
}

/// A selectable option: stable value + human label + optional hint. Mirrors the
/// web `Option` shape (kept local to the pure layer so it needs no UI import).
struct StackOption: Hashable {
    var value: String
    var label: String
    var hint: String = ""
}

/// The real `AutonomyLevel` ladder (`crates/lopi-core/src/loop_config.rs`).
let AUTONOMY_OPTIONS: [StackOption] = [
    StackOption(value: "L1", label: "L1 · Report only", hint: "report only, no PR"),
    StackOption(value: "L2", label: "L2 · Draft PR", hint: "draft PR, human approves"),
    StackOption(value: "L3", label: "L3 · Verified PR", hint: "verify before PR"),
    StackOption(value: "L4", label: "L4 · Auto-merge", hint: "auto-merge on pass")
]

/// Placeholder branch list — there is no `/api/branches` seed in the pure
/// layer, so this is a static convenience (same honesty caveat as web).
let BRANCH_OPTIONS: [StackOption] = [
    StackOption(value: "main", label: "main"),
    StackOption(value: "dev", label: "dev")
]

/// Worker-model options — the same catalog `LaunchControls.models` carries, kept
/// here so the pure layer's `DEFAULT_STACK_DEFAULTS.model` matches the app.
let MODEL_OPTIONS: [StackOption] = [
    StackOption(value: "claude-opus-4-8", label: "Opus 4.8", hint: "deepest"),
    StackOption(value: "claude-sonnet-4-6", label: "Sonnet 4.6", hint: "balanced"),
    StackOption(value: "claude-haiku-4-5", label: "Haiku 4.5", hint: "fastest")
]

/// Reasoning-effort options.
let EFFORT_OPTIONS: [StackOption] = [
    StackOption(value: "low", label: "Low"),
    StackOption(value: "medium", label: "Medium"),
    StackOption(value: "high", label: "High"),
    StackOption(value: "max", label: "Max")
]

/// The app-wide `DEF` a stack's own defaults start from and are compared
/// against (`stackDefaultsActive`).
let DEFAULT_STACK_DEFAULTS = StackDefaults(
    model: MODEL_OPTIONS[0].value,
    effort: "medium",
    repo: "",
    branch: BRANCH_OPTIONS[0].value,
    autonomy: "L2"
)

/// Fresh defaults for a newly-created stack (value type — no shared reference).
func defaultStackDefaults() -> StackDefaults { DEFAULT_STACK_DEFAULTS }

// MARK: - Chain-scope guardrails / goal

/// The chain-level analogue of a loop's `Guardrails` — deliberately narrower
/// (no gate/until: those are shell pre/exit conditions around a *single* task's
/// retry loop, with nowhere to run at chain scope). `onFail` is WIRED into the
/// chain sequencer; `budget` stays client-only.
struct StackGuardrails: Codable, Hashable {
    var onFail: OnFail
    var budget: Budget
}

/// Freshly-initialized chain guardrails.
func defaultStackGuardrails() -> StackGuardrails {
    StackGuardrails(onFail: .stop, budget: .auto)
}

/// The stack control area's placement. `dock` is the shipped default (a
/// collapsible strip); `sticky` is the always-expanded variant whose CSS ships
/// unused — flipping this constant is the whole migration (the `SIDEBAR_MODE`
/// precedent). Not user-facing this sprint.
enum StackControlMode { case dock, sticky }
let STACK_CONTROL_MODE: StackControlMode = .dock

/// A chain run's default iteration count — `1` (run once through), not the
/// per-loop `DEFAULT_MAX_ITERATIONS`. Reuses the `0` = infinite sentinel.
let DEFAULT_STACK_LOOP_COUNT = 1

/// B1 — default no-progress tolerance for a goal-pursuing stack.
let DEFAULT_NO_PROGRESS_LIMIT = 3

/// B1 — the stack's run-until-goal facet. Off by default (additive/backward-
/// compatible).
struct StackGoal: Codable, Hashable {
    /// Run-until-goal on/off.
    var pursue: Bool
    /// Consecutive non-gaining chain-runs tolerated before a `no_progress` stop;
    /// `0` disables the no-progress detector.
    var noProgressLimit: Int
}

/// Freshly-initialized goal facet.
func defaultStackGoal() -> StackGoal {
    StackGoal(pursue: false, noProgressLimit: DEFAULT_NO_PROGRESS_LIMIT)
}

// MARK: - Stack config (the purple control area's full state)

/// Stack-level config. `scheduled`/`cron` are STUBBED (no whole-chain cron
/// server-side). `evals` is CLIENT-ONLY chain-acceptance intent. `defaults` is
/// WIRED (resolved into every loop's payload). `goal` is B1 run-until-goal.
struct StackConfig: Codable, Hashable {
    var loopCount: Int
    var scheduled: Bool
    var cron: CronConfig
    var guardrails: StackGuardrails
    var evals: [EvalRef]
    var defaults: StackDefaults
    var goal: StackGoal
}

/// Freshly-initialized stack config — every pane gets its own value.
func defaultStackConfig() -> StackConfig {
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
struct StackPaneState: Codable, Hashable, Identifiable {
    var key: String
    var title: String
    var cards: [StackCard]
    var config: StackConfig

    var id: String { key }
}

// MARK: - Pane-level defaults a card's config falls back to (the 3 WIRED fields)

/// Pane-level defaults a card's `config` overrides fall back to. A superset —
/// `StackDefaults` — is accepted anywhere this is (the production shape).
struct PaneDefaults {
    var model: String
    var effort: String
    var repo: String
}

extension PaneDefaults {
    /// Resolve from a full `StackDefaults` — the production call shape, where a
    /// pane's `config.defaults` supplies all three WIRED fields.
    init(_ d: StackDefaults) {
        self.init(model: d.model, effort: d.effort, repo: d.repo)
    }
}
