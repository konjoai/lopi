import Foundation
import Observation

// The pane store — the `panes` writable analogue from `stores/stack.ts`. Holds
// the in-memory pane list and the keyed-dispatch mutation wrappers over the pure
// ops. `@Observable` so SwiftUI re-renders on change; ZERO SwiftUI/AppKit
// imports (Foundation + Observation only) so it lifts into a shared package
// unchanged, per the macOS-Loop-Stacks brief's pure-Swift-types rule.

@Observable
@MainActor
public final class StackStore {
    /// The active stack panes — client-only, in-memory, no persistence this
    /// slice (a relaunch loses pane state, exactly like web loses it on reload).
    public private(set) var panes: [StackPaneState]

    public init(panes: [StackPaneState]? = nil) {
        self.panes = panes ?? Self.defaultPanes()
    }

    public static func defaultPanes() -> [StackPaneState] {
        [
            StackPaneState(key: "s1", title: "stack one", cards: [], config: defaultStackConfig()),
            StackPaneState(key: "s2", title: "stack two", cards: [], config: defaultStackConfig())
        ]
    }

    public func pane(for key: String) -> StackPaneState? {
        panes.first { $0.key == key }
    }

    // MARK: Card ops (keyed dispatch over the pure array ops)

    public func addToPane(_ key: String, _ card: StackCard) {
        panes = applyToPaneCards(panes, key) { addCard($0, card) }
    }
    public func removeFromPane(_ key: String, _ id: String) {
        panes = applyToPaneCards(panes, key) { removeCard($0, id) }
    }
    public func duplicateInPane(_ key: String, _ id: String) {
        panes = applyToPaneCards(panes, key) { duplicateCard($0, id) }
    }
    public func reorderInPane(_ key: String, _ from: Int, _ to: Int) {
        panes = applyToPaneCards(panes, key) { reorderCard($0, from, to) }
    }
    public func reorderInPaneRelative(_ key: String, _ fromIndex: Int, _ targetIndex: Int, _ before: Bool) {
        panes = applyToPaneCards(panes, key) { moveCardBeforeOrAfter($0, fromIndex, targetIndex, before) }
    }
    public func insertCardIntoPane(_ key: String, _ index: Int, _ card: StackCard) {
        panes = insertIntoPane(panes, key, index, card)
    }

    /// Patch a single card by id (whole-field mutation via a closure — the Swift
    /// analogue of web's shallow-merge `Partial<StackCard>`).
    public func updateCardInPane(_ key: String, _ id: String, _ mutate: (inout StackCard) -> Void) {
        panes = applyToPaneCards(panes, key) { patchCard($0, id, mutate) }
    }

    // MARK: Draft ops (Creation-Flow-1)

    /// Patch a pane's draft card. The draft is edited in place until committed
    /// via `commitDraft`. No-op for an unknown key. Mirrors `updateDraftInPane`.
    public func updateDraftInPane(_ key: String, _ mutate: (inout StackCard) -> Void) {
        guard let idx = panes.firstIndex(where: { $0.key == key }) else { return }
        mutate(&panes[idx].draft)
    }

    /// Commit a pane's draft into a real (`.idle`) card at the top of the stack
    /// (`addCard` prepends), then mint a fresh empty draft. The one transition a
    /// draft ever makes out of `.draft`. No-op for an unknown key.
    public func commitDraft(_ key: String) {
        guard let idx = panes.firstIndex(where: { $0.key == key }) else { return }
        panes[idx].cards = addCard(panes[idx].cards, finalizeDraft(panes[idx].draft))
        panes[idx].draft = makeDraft()
    }

    /// Replace a pane's draft with a fresh empty one.
    public func resetDraft(_ key: String) {
        guard let idx = panes.firstIndex(where: { $0.key == key }) else { return }
        panes[idx].draft = makeDraft()
    }

    /// Drop a whole stack template into a pane at once, in the correct run order
    /// (`applyStackTemplate` — first loop at the bottom).
    public func applyStackTemplateToPane(_ key: String, _ tpl: StackTemplate) {
        panes = applyToPaneCards(panes, key) { applyStackTemplate(tpl, into: $0) }
    }

    // MARK: Stack-level config + pane ops

    /// Patch a pane's stack-level config via a mutating closure.
    public func updateStackConfig(_ key: String, _ mutate: (inout StackConfig) -> Void) {
        guard let idx = panes.firstIndex(where: { $0.key == key }) else { return }
        mutate(&panes[idx].config)
    }

    public func duplicateStackInPanes(_ key: String) {
        panes = duplicateStack(panes, key)
    }
    public func loadStackCardsIntoPane(_ targetKey: String, _ sourceKey: String) {
        panes = loadStackCardsInto(panes, targetKey: targetKey, sourceKey: sourceKey)
    }
    public func reorderStacksInPanes(_ fromIndex: Int, _ targetIndex: Int, _ before: Bool) {
        panes = moveStackBeforeOrAfter(panes, fromIndex, targetIndex, before)
    }
    public func deleteStackFromPanes(_ key: String) {
        panes = deleteStack(panes, key)
    }
    public func addStackPane() {
        panes = addStack(panes)
    }
}
