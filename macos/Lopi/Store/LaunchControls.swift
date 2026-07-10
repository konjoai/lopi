import Foundation
import Observation

/// A selectable option: stable value + human label + optional hint.
struct LaunchOption: Identifiable, Hashable {
    let value: String
    let label: String
    var hint: String = ""
    var id: String { value }
}

/// Launch controls shared by every empty pane — model / effort / priority and
/// the repo / branch overrides. Persisted so the cockpit remembers the last
/// setup. The native counterpart of the web `controls` store.
@Observable
@MainActor
final class LaunchControls {
    static let models: [LaunchOption] = [
        .init(value: "claude-opus-4-8", label: "Opus 4.8", hint: "deepest"),
        .init(value: "claude-sonnet-4-6", label: "Sonnet 4.6", hint: "balanced"),
        .init(value: "claude-haiku-4-5", label: "Haiku 4.5", hint: "fastest")
    ]
    static let efforts: [LaunchOption] = [
        .init(value: "low", label: "Low"),
        .init(value: "medium", label: "Medium"),
        .init(value: "high", label: "High"),
        .init(value: "max", label: "Max")
    ]
    static let priorities: [LaunchOption] = [
        .init(value: "low", label: "Low"),
        .init(value: "normal", label: "Normal"),
        .init(value: "high", label: "High"),
        .init(value: "critical", label: "Critical")
    ]

    var model: String { didSet { persist() } }
    var effort: String { didSet { persist() } }
    var priority: String { didSet { persist() } }
    var repo: String { didSet { persist() } }
    var branch: String { didSet { persist() } }

    @ObservationIgnored private let defaults: UserDefaults

    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
        model = defaults.string(forKey: "lopi.lc.model") ?? Self.models[0].value
        effort = defaults.string(forKey: "lopi.lc.effort") ?? "medium"
        priority = defaults.string(forKey: "lopi.lc.priority") ?? "normal"
        repo = defaults.string(forKey: "lopi.lc.repo") ?? ""
        branch = defaults.string(forKey: "lopi.lc.branch") ?? ""
    }

    /// Build the `CreateTaskBody` for a goal. Model and effort ride the real
    /// `model` / `effort` request fields the backend's `select_model` honors
    /// verbatim, so the model that runs matches the one the pane shows (Ops-2
    /// finding #7). They used to be folded into free-text planning constraints
    /// the runner ignored, which is why a small task silently ran on the
    /// heuristic default (Haiku) despite the pane displaying the picked model.
    /// Branch has no `CreateTaskRequest` field, so it stays a constraint.
    func body(goal: String, repoOverride: String? = nil) -> CreateTaskBody {
        var constraints: [String] = []
        if !branch.isEmpty { constraints.append("Target branch: \(branch)") }
        let resolvedRepo = (repoOverride?.isEmpty == false ? repoOverride : nil) ?? (repo.isEmpty ? nil : repo)
        return CreateTaskBody(
            goal: goal,
            repo: resolvedRepo,
            priority: priority,
            constraints: constraints.isEmpty ? nil : constraints,
            allowedDirs: nil,
            forbiddenDirs: nil,
            maxRetries: nil,
            model: model.isEmpty ? nil : model,
            effort: effort.isEmpty ? nil : effort
        )
    }

    private func persist() {
        defaults.set(model, forKey: "lopi.lc.model")
        defaults.set(effort, forKey: "lopi.lc.effort")
        defaults.set(priority, forKey: "lopi.lc.priority")
        defaults.set(repo, forKey: "lopi.lc.repo")
        defaults.set(branch, forKey: "lopi.lc.branch")
    }

    static func label(_ options: [LaunchOption], _ value: String) -> String {
        options.first { $0.value == value }?.label ?? value
    }
}
