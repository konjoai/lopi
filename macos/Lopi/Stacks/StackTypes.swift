import Foundation

// Loop-stack domain types — the pure Swift port of the web `stores/stack.ts`
// type layer (+ `stackDefaults.ts`). ZERO SwiftUI/AppKit imports on purpose:
// these are plain structs/enums/logic so a future shared-package extraction
// (the open question in iOS-Research-1) is a move, not a rewrite. Foundation
// only, for `UUID`/`Date`. The pure ops live in `StackOps.swift`/
// `StackCron.swift`/`StackPayload.swift`/`StackPaneOps.swift`; the observable
// store wrapper (the `panes`/`runs` analogue) lives in `StackStore.swift`.
//
// Mirrors the shipped, tested web code 1:1 — where a design doc disagreed with
// the shipped web, the shipped web won (per the macOS-Loop-Stacks brief).

// MARK: - Evals

/// One rung of the eval ladder a card carries.
enum EvalTier: String, Codable, Hashable {
    case base, test, judge, suite
}

/// A single named eval, either the full catalog or a card's on-set.
struct EvalRef: Codable, Hashable {
    var name: String
    var tier: EvalTier
}

// MARK: - Presets

/// The five built-in presets a card can be created from.
enum PresetKey: String, Codable, Hashable, CaseIterable {
    case research, implement, optimize, gain, benchmark
}

/// A preset's fixed shape: its alias, keyword-suggestion triggers, and the eval
/// suite it carries (baseline always first).
struct PresetDef {
    var key: PresetKey
    var label: String
    var alias: String
    var keywords: [String]
    var evals: [EvalRef]
}

// MARK: - Guardrails / budget

/// Policy applied when a card's loop iteration fails. Mirrors `OnFail`
/// (`crates/lopi-core/src/loop_config.rs`) — WIRED via `on_fail`.
enum OnFail: String, Codable, Hashable {
    case stop, `continue`, backoff
}

/// Per-run token-budget preset. Wired to the real `budget_tokens` field via
/// `budgetToTokens`; `auto`/`none` set no hard cap.
enum Budget: String, Codable, Hashable {
    case auto
    case k200 = "200k"
    case none
}

/// A card's run-limit guardrails. `gate`/`until`/`onFail` are WIRED to the real
/// `CreateTaskOptions.gate` / `.until` / `.on_fail` fields.
struct Guardrails: Codable, Hashable {
    var gate: Bool
    var gateCmd: String
    var until: Bool
    var untilCmd: String
    var onFail: OnFail
    /// Backend gap: no budget field exists on `CreateTaskRequest` yet.
    var budget: Budget
}

/// Freshly-initialized guardrails — every card gets its own value (structs are
/// value types, so no shared-reference hazard the web comment guards against).
func defaultGuardrails() -> Guardrails {
    Guardrails(gate: false, gateCmd: "", until: false, untilCmd: "", onFail: .stop, budget: .auto)
}

// MARK: - Cron

/// The five preset schedule cadences a card can pick, plus a raw-cron escape.
enum CronFreq: String, Codable, Hashable, CaseIterable {
    case everyMinute = "every minute"
    case hourly, daily, weekly, custom
}

/// Three-letter weekday tags, matching cron's day-of-week vocabulary.
enum Dow: String, Codable, Hashable, CaseIterable {
    case Sun, Mon, Tue, Wed, Thu, Fri, Sat
}

/// A card's schedule. `raw` is the standard 5-field cron string — WIRED. The
/// preset fields are the two-way-synced UI state `raw` derives from.
struct CronConfig: Codable, Hashable {
    var freq: CronFreq
    var hour12: Int
    var min: Int
    var ampm: AmPm
    var dow: Dow
    var raw: String

    enum AmPm: String, Codable, Hashable { case AM, PM }
}

/// Freshly-initialized cron config — daily 2 AM.
func defaultCron() -> CronConfig {
    CronConfig(freq: .daily, hour12: 2, min: 0, ampm: .AM, dow: .Mon, raw: "0 2 * * *")
}

// MARK: - Card config

/// Per-loop overrides of the pane defaults. `nil` on any field means "inherit
/// the pane default". `model`/`effort`/`repo` are WIRED; `branch`/`autonomy`
/// are client-only.
struct CardConfig: Codable, Hashable {
    var model: String?
    var effort: String?
    var repo: String?
    var branch: String?
    var autonomy: String?

    init(model: String? = nil, effort: String? = nil, repo: String? = nil,
         branch: String? = nil, autonomy: String? = nil) {
        self.model = model
        self.effort = effort
        self.repo = repo
        self.branch = branch
        self.autonomy = autonomy
    }
}

/// A card's lifecycle state. `draft` is the pre-commit state of the pane's
/// in-composer draft card (Creation-Flow-1) — never in `pane.cards`, excluded
/// from every run/loop-count/payload path (see `executionOrder`), and handled
/// explicitly by every `CardStatus` switch rather than falling into a run path.
enum CardStatus: String, Codable, Hashable {
    case draft, idle, queued, running, done
}

/// Which kind of template produced a card — drives the provenance chip's color
/// (`prompt` → sun chip replacing the alias chip; `stack` → violet chip
/// alongside the alias chip). Set iff `StackCard.tpl` is set.
enum TplKind: String, Codable, Hashable {
    case prompt, stack
}

/// The default iteration ceiling a fresh card starts from. `0` = "off": the
/// loop is disabled and the card runs a single pass (the card pill floors at 0
/// and never reaches the backend's infinite sentinel — the payload maps an off
/// card to a single `max_iterations: 1`). A user dials this *up* to ask for
/// repeats.
let DEFAULT_MAX_ITERATIONS = 0
/// Floor the *stack* loop-count stepper will not go below without wrapping to
/// infinite. The card pill uses `stepCardIterations` and ignores this.
let MAX_ITERATIONS_FLOOR = 2

/// Live iteration progress while a card runs.
struct IterationProgress: Codable, Hashable {
    var current: Int
    var total: Int
}

/// One card in the stack — a loop-to-be.
struct StackCard: Codable, Hashable, Identifiable {
    var id: String
    var preset: PresetKey?
    var goal: String
    var alias: String?
    var literal: Bool
    var evals: [EvalRef]
    var status: CardStatus
    /// Hard iteration ceiling. `0` = infinite (mirrors the backend sentinel).
    var maxIterations: Int
    var iteration: IterationProgress?
    var scheduled: Bool
    var cron: CronConfig
    var guardrails: Guardrails
    var config: CardConfig
    var taskId: String?
    /// Name of the template this card came from (provenance, not a binding).
    /// Records origin only — it survives edits to `goal`/`preset` and never
    /// tracks drift. `nil` when the card came from no template.
    var tpl: String? = nil
    /// Which kind of template produced it — drives the provenance chip's color.
    /// Set iff `tpl` is set.
    var tplKind: TplKind? = nil
}

// MARK: - Eval catalog (client-side static config)

/// Baseline eval — always present, on every card.
let BASELINE_EVAL = EvalRef(name: "execution ok", tier: .base)

/// The full pickable eval catalog. Baseline first and locked-on.
let EVAL_CATALOG: [EvalRef] = [
    BASELINE_EVAL,
    EvalRef(name: "tests pass", tier: .test),
    EvalRef(name: "unit", tier: .test),
    EvalRef(name: "integration", tier: .test),
    EvalRef(name: "benchmark gate", tier: .test),
    EvalRef(name: "30-run gate", tier: .test),
    EvalRef(name: "code review", tier: .judge),
    EvalRef(name: "beats-best", tier: .judge),
    EvalRef(name: "vuln scan", tier: .suite),
    EvalRef(name: "adversarial", tier: .suite)
]

/// Suite shortcuts — clicking one turns on every named eval.
let EVAL_SUITES: [String: [String]] = [
    "kcqf": ["tests pass", "code review", "vuln scan", "adversarial"],
    "security": ["vuln scan", "adversarial"],
    "research": ["code review"]
]

/// The preset catalog, keyed by `PresetKey`.
let PRESET_CATALOG: [PresetKey: PresetDef] = [
    .research: PresetDef(
        key: .research, label: "research", alias: ":research",
        keywords: ["research", "investigate", "explore", "learn", "study", "survey"],
        evals: [BASELINE_EVAL, EvalRef(name: "code review", tier: .judge)]),
    .implement: PresetDef(
        key: .implement, label: "implement", alias: ":implement",
        keywords: ["add", "build", "implement", "feature", "create", "gate", "wire"],
        evals: [
            BASELINE_EVAL,
            EvalRef(name: "unit", tier: .test),
            EvalRef(name: "integration", tier: .test),
            EvalRef(name: "code review", tier: .judge),
            EvalRef(name: "vuln scan", tier: .suite),
            EvalRef(name: "adversarial", tier: .suite)
        ]),
    .optimize: PresetDef(
        key: .optimize, label: "optimize", alias: ":optimize",
        keywords: ["optimize", "improve", "speed", "performance", "faster", "latency"],
        evals: [
            BASELINE_EVAL,
            EvalRef(name: "beats-best", tier: .judge),
            EvalRef(name: "30-run gate", tier: .test),
            EvalRef(name: "adversarial", tier: .suite)
        ]),
    .gain: PresetDef(
        key: .gain, label: "gain", alias: ":gain",
        keywords: ["gain", "ratchet", "self-improve", "self improve", "beats-best"],
        evals: [
            BASELINE_EVAL,
            EvalRef(name: "beats-best", tier: .judge),
            EvalRef(name: "adversarial", tier: .suite)
        ]),
    .benchmark: PresetDef(
        key: .benchmark, label: "benchmark", alias: ":benchmark",
        keywords: ["benchmark", "measure", "variance", "throughput"],
        evals: [
            BASELINE_EVAL,
            EvalRef(name: "benchmark gate", tier: .test),
            EvalRef(name: "30-run gate", tier: .test)
        ])
]

/// Ordered preset keys (declaration order matters for `suggestPreset`).
let PRESET_KEYS: [PresetKey] = [.research, .implement, .optimize, .gain, .benchmark]

/// One-line human descriptions for the templates menu's presets section
/// (Creation-Flow-1 §5). Kept beside the catalog so web + macOS read the same
/// copy (mirrors the web `PRESET_DESCRIPTIONS`).
let PRESET_DESCRIPTIONS: [PresetKey: String] = [
    .research: "explore & investigate — judge-reviewed",
    .implement: "build a feature — full test + review suite",
    .optimize: "improve speed — beats-best + 30-run gate",
    .gain: "self-improve — ratchet on beats-best",
    .benchmark: "measure variance — benchmark + 30-run gate"
]

/// Legacy `:alias` tokens mapping onto a renamed preset key. `:ratchet` → `:gain`.
let LEGACY_ALIASES: [String: PresetKey] = ["ratchet": .gain]
