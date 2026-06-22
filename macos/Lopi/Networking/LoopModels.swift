import Foundation

/// Loop Engineering snapshot from `GET /api/loop-engineering` — the macOS
/// mirror of the web Loop screen's payload. One read-only view over every
/// loop-engineering lever for the primary repo.
struct LoopSnapshot: Codable, Equatable {
    var repo: String
    var config: LoopConfigDTO
    var autonomyLevels: [LoopAutonomyOption]
    var selfPromptStrategies: [LoopSelfPromptOption]
    var skills: [LoopSkill]
    var rules: [LoopRule]
    var schedules: [LoopSchedule]
    var gates: [LoopGate]

    enum CodingKeys: String, CodingKey {
        case repo, config, skills, rules, schedules, gates
        case autonomyLevels = "autonomy_levels"
        case selfPromptStrategies = "self_prompt_strategies"
    }
}

/// Effective `.lopi/loop.toml` plus its validation envelope.
struct LoopConfigDTO: Codable, Equatable {
    var autonomyLevel: String
    var autonomyTag: String
    var autonomyLabel: String
    var selfPrompt: String
    var selfPromptTag: String
    var selfPromptLabel: String
    var escalateStrategy: Bool
    var escalationLadder: [LoopEscalationRung]
    var visionPath: String?
    var skillsEnabled: [String]
    var rulesEnabled: [String]
    var noProgressLimit: Int
    var maxIterations: Int
    var budgetTokens: Int
    var valid: Bool
    var issues: [String]

    enum CodingKeys: String, CodingKey {
        case valid, issues
        case autonomyLevel = "autonomy_level"
        case autonomyTag = "autonomy_tag"
        case autonomyLabel = "autonomy_label"
        case selfPrompt = "self_prompt"
        case selfPromptTag = "self_prompt_tag"
        case selfPromptLabel = "self_prompt_label"
        case escalateStrategy = "escalate_strategy"
        case escalationLadder = "escalation_ladder"
        case visionPath = "vision_path"
        case skillsEnabled = "skills_enabled"
        case rulesEnabled = "rules_enabled"
        case noProgressLimit = "no_progress_limit"
        case maxIterations = "max_iterations"
        case budgetTokens = "budget_tokens"
    }
}

/// One rung of the adaptive-escalation ladder: which strategy attempt N uses.
struct LoopEscalationRung: Codable, Equatable, Identifiable {
    var attempt: Int
    var tag: String
    var label: String

    var id: Int { attempt }
}

/// One pickable self-prompting strategy (S1–S4) with a live self-prompt preview.
struct LoopSelfPromptOption: Codable, Equatable, Identifiable {
    var value: String
    var tag: String
    var label: String
    var description: String
    var preview: String

    var id: String { value }
}

/// One pickable rung on the L1–L4 autonomy ladder.
struct LoopAutonomyOption: Codable, Equatable, Identifiable {
    var value: String
    var tag: String
    var label: String
    var opensPr: Bool
    var requiresVerifier: Bool
    var allowsAutoMerge: Bool

    var id: String { value }

    enum CodingKeys: String, CodingKey {
        case value, tag, label
        case opensPr = "opens_pr"
        case requiresVerifier = "requires_verifier"
        case allowsAutoMerge = "allows_auto_merge"
    }
}

/// A discovered skill (`.claude/skills/<name>/SKILL.md`).
struct LoopSkill: Codable, Equatable, Identifiable {
    var name: String
    var description: String
    var id: String { name }
}

/// A discovered rule file (`.claude/rules/<name>.md`).
struct LoopRule: Codable, Equatable, Identifiable {
    var name: String
    var id: String { name }
}

/// A schedule projected for the Loop screen, carrying its trust level.
struct LoopSchedule: Codable, Equatable, Identifiable {
    var id: String
    var name: String
    var goal: String
    var cron: String
    var enabled: Bool
    var autonomyLevel: String
    var autonomyTag: String
    var autonomyLabel: String

    enum CodingKeys: String, CodingKey {
        case id, name, goal, cron, enabled
        case autonomyLevel = "autonomy_level"
        case autonomyTag = "autonomy_tag"
        case autonomyLabel = "autonomy_label"
    }
}

/// A Konjo quality wall surfaced as a loop guardrail gate.
struct LoopGate: Codable, Equatable, Identifiable {
    var wall: String
    var name: String
    var checks: String
    var id: String { wall }
}

// MARK: - Loop Health

/// Loop-health snapshot from `GET /api/loop-engineering/health` — the
/// observability surface over data the loop already persists (attempts,
/// turn metrics, verifier verdicts). The macOS mirror of the web payload.
struct LoopHealth: Codable, Equatable {
    var stats: LoopHealthStats
    var attempts: [LoopHealthAttempt]
    var outcomes: [LoopOutcome]
    var burn: [LoopBurn]
}

/// Headline KPI tiles for the Loop Health view.
struct LoopHealthStats: Codable, Equatable {
    var runs: Int
    var attempts: Int
    var successRate: Double
    var verifierPassRate: Double
    var verifierTotal: Int
    var spendUsd: Double
    var tokens: Int

    enum CodingKeys: String, CodingKey {
        case runs, attempts, tokens
        case successRate = "success_rate"
        case verifierPassRate = "verifier_pass_rate"
        case verifierTotal = "verifier_total"
        case spendUsd = "spend_usd"
    }
}

/// One attempt in the score/diff timeline (oldest → newest).
struct LoopHealthAttempt: Codable, Equatable {
    var taskId: String
    var attempt: Int
    var testPassRate: Double
    var lintErrors: Int
    var diffLines: Int
    var outcome: String
    var createdAt: String

    enum CodingKeys: String, CodingKey {
        case attempt, outcome
        case taskId = "task_id"
        case testPassRate = "test_pass_rate"
        case lintErrors = "lint_errors"
        case diffLines = "diff_lines"
        case createdAt = "created_at"
    }
}

/// One slice of the attempt-outcome distribution.
struct LoopOutcome: Codable, Equatable, Identifiable {
    var label: String
    var count: Int
    var id: String { label }
}

/// One sample in the token/cost burn series (oldest → newest).
struct LoopBurn: Codable, Equatable {
    var costUsd: Double
    var tokens: Int
    var contextPressure: Double
    var timestamp: String

    enum CodingKeys: String, CodingKey {
        case tokens, timestamp
        case costUsd = "cost_usd"
        case contextPressure = "context_pressure"
    }
}

// MARK: - Per-run drill-down

/// One run (task) summarised for the run picker.
struct LoopRun: Codable, Equatable, Identifiable {
    var taskId: String
    var goal: String
    var status: String
    var attempts: Int
    var bestScore: Double
    var finalOutcome: String
    var lastAt: String
    var id: String { taskId }

    enum CodingKeys: String, CodingKey {
        case goal, status, attempts
        case taskId = "task_id"
        case bestScore = "best_score"
        case finalOutcome = "final_outcome"
        case lastAt = "last_at"
    }
}

/// The run-list envelope (`{ "runs": [...] }`).
struct LoopRunList: Codable, Equatable {
    var runs: [LoopRun]
}

/// The verifier verdict grafted onto an attempt in a run trace.
struct LoopRunVerifier: Codable, Equatable {
    var passed: Bool
    var confidence: Double
    var gaps: [String]
    var fixHints: [String]

    enum CodingKeys: String, CodingKey {
        case passed, confidence, gaps
        case fixHints = "fix_hints"
    }
}

/// One attempt in a run's drill-down trace.
struct LoopRunAttempt: Codable, Equatable, Identifiable {
    var attempt: Int
    var testPassRate: Double
    var lintErrors: Int
    var diffLines: Int
    var outcome: String
    var errors: [String]
    var verifier: LoopRunVerifier?
    var tokens: Int
    var costUsd: Double
    var createdAt: String
    var id: Int { attempt }

    enum CodingKeys: String, CodingKey {
        case attempt, outcome, errors, verifier, tokens
        case testPassRate = "test_pass_rate"
        case lintErrors = "lint_errors"
        case diffLines = "diff_lines"
        case costUsd = "cost_usd"
        case createdAt = "created_at"
    }
}

/// A single run's attempt-by-attempt trace.
struct LoopRunTrace: Codable, Equatable {
    var taskId: String
    var goal: String
    var status: String
    var attempts: [LoopRunAttempt]

    enum CodingKeys: String, CodingKey {
        case goal, status, attempts
        case taskId = "task_id"
    }
}
