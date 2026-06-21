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
