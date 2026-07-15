import Foundation
import Observation

// Template persistence (Creation-Flow-1 §2) — an @Observable store backed by
// **UserDefaults** under `lopi.templates.v1`, the same key name and JSON shape
// as the web's localStorage store.
//
// PER-MACHINE, CLIENT-ONLY, NOT DURABLE OR SHARED. web (localStorage) and macOS
// (UserDefaults) keep *separate* template libraries — they do NOT sync. Same
// conceptual shape, different physical store. Decoding is defensive: a missing
// or corrupt value degrades to empty and never crashes. Seeds a couple of
// templates only when the key is absent (first launch on this machine).

/// The persisted shape under `lopi.templates.v1` — mirrors the web
/// `{ prompts: [...], stacks: [...] }`.
public struct TemplateLibrary: Codable {
    public var prompts: [PromptTemplate]
    public var stacks: [StackTemplate]

    public init(prompts: [PromptTemplate], stacks: [StackTemplate]) {
        self.prompts = prompts
        self.stacks = stacks
    }

    public static let empty = TemplateLibrary(prompts: [], stacks: [])
}

@Observable
@MainActor
public final class StackTemplateStore {
    private static let storageKey = "lopi.templates.v1"
    private let defaults: UserDefaults

    /// The live template library, mirrored to UserDefaults on every mutation.
    public private(set) var library: TemplateLibrary

    public init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
        self.library = Self.load(from: defaults)
    }

    /// Append a prompt template and persist.
    public func savePrompt(_ tpl: PromptTemplate) {
        library.prompts.append(tpl)
        persist()
    }

    /// Append a stack template and persist.
    public func saveStack(_ tpl: StackTemplate) {
        library.stacks.append(tpl)
        persist()
    }

    // MARK: - Persistence (defensive, never throws into a caller)

    private func persist() {
        do {
            let data = try JSONEncoder().encode(library)
            defaults.set(data, forKey: Self.storageKey)
        } catch {
            // No silent failure: surface why it didn't persist, but never throw
            // into the caller's action. Client-only, not durable — see the doc.
            print("lopi: could not persist templates (per-machine, not durable): \(error)")
        }
    }

    /// Read the persisted library, or fall back. Seeds on first launch (absent
    /// key). Never crashes — a corrupt value yields an empty library.
    private static func load(from defaults: UserDefaults) -> TemplateLibrary {
        guard let data = defaults.data(forKey: storageKey) else {
            let seeded = seed()
            if let encoded = try? JSONEncoder().encode(seeded) {
                defaults.set(encoded, forKey: storageKey)
            }
            return seeded
        }
        guard let decoded = try? JSONDecoder().decode(TemplateLibrary.self, from: data) else {
            return .empty  // corrupt JSON → empty, never crash
        }
        return decoded
    }

    /// Seed templates written only when the key is absent — a couple of starting
    /// points so the menu isn't empty on a fresh machine. Ids are static (seeds,
    /// not minted) so a re-seed can't collide. Mirrors the web seeds exactly.
    private static func seed() -> TemplateLibrary {
        TemplateLibrary(
            prompts: [
                PromptTemplate(id: "seed-prompt-research", name: "deep research", preset: .research, alias: nil,
                               goal: "investigate the problem space and summarize findings"),
                PromptTemplate(id: "seed-prompt-implement", name: "ship a feature", preset: .implement, alias: nil,
                               goal: "implement the change end-to-end with tests")
            ],
            stacks: [
                StackTemplate(id: "seed-stack-kcqf", name: "kcqf sprint", loops: [
                    // Serialized bottom-first (run order): research runs first,
                    // then implement, then optimize.
                    TemplateLoop(preset: .research, alias: nil, goal: "research the problem space"),
                    TemplateLoop(preset: .implement, alias: nil, goal: "implement the change"),
                    TemplateLoop(preset: .optimize, alias: nil, goal: "optimize and harden")
                ])
            ])
    }
}
