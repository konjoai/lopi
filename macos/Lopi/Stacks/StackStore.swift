import Foundation
import Observation

// The pane store — the `panes` writable analogue from `stores/stack.ts`. Holds
// the in-memory pane list and the keyed-dispatch mutation wrappers over the pure
// ops. `@Observable` so SwiftUI re-renders on change; ZERO SwiftUI/AppKit
// imports (Foundation + Observation only) so it lifts into a shared package
// unchanged, per the macOS-Loop-Stacks brief's pure-Swift-types rule.

@Observable
@MainActor
final class StackStore {
    /// The active stack panes — client-only, in-memory, no persistence this
    /// slice (a relaunch loses pane state, exactly like web loses it on reload).
    private(set) var panes: [StackPaneState]

    init(panes: [StackPaneState]? = nil) {
        self.panes = panes ?? Self.defaultPanes()
    }

    static func defaultPanes() -> [StackPaneState] {
        [
            StackPaneState(key: "s1", title: "stack one", cards: [], config: defaultStackConfig()),
            StackPaneState(key: "s2", title: "stack two", cards: [], config: defaultStackConfig())
        ]
    }

    func pane(for key: String) -> StackPaneState? {
        panes.first { $0.key == key }
    }

    // MARK: Card ops (keyed dispatch over the pure array ops)

    func addToPane(_ key: String, _ card: StackCard) {
        panes = applyToPaneCards(panes, key) { addCard($0, card) }
    }
    func removeFromPane(_ key: String, _ id: String) {
        panes = applyToPaneCards(panes, key) { removeCard($0, id) }
    }
    func duplicateInPane(_ key: String, _ id: String) {
        panes = applyToPaneCards(panes, key) { duplicateCard($0, id) }
    }
    func reorderInPane(_ key: String, _ from: Int, _ to: Int) {
        panes = applyToPaneCards(panes, key) { reorderCard($0, from, to) }
    }
    func reorderInPaneRelative(_ key: String, _ fromIndex: Int, _ targetIndex: Int, _ before: Bool) {
        panes = applyToPaneCards(panes, key) { moveCardBeforeOrAfter($0, fromIndex, targetIndex, before) }
    }
    func insertCardIntoPane(_ key: String, _ index: Int, _ card: StackCard) {
        panes = insertIntoPane(panes, key, index, card)
    }

    /// Patch a single card by id (whole-field mutation via a closure — the Swift
    /// analogue of web's shallow-merge `Partial<StackCard>`).
    func updateCardInPane(_ key: String, _ id: String, _ mutate: (inout StackCard) -> Void) {
        panes = applyToPaneCards(panes, key) { patchCard($0, id, mutate) }
    }

    // MARK: Stack-level config + pane ops

    /// Patch a pane's stack-level config via a mutating closure.
    func updateStackConfig(_ key: String, _ mutate: (inout StackConfig) -> Void) {
        guard let idx = panes.firstIndex(where: { $0.key == key }) else { return }
        mutate(&panes[idx].config)
    }

    func duplicateStackInPanes(_ key: String) {
        panes = duplicateStack(panes, key)
    }
    func reorderStacksInPanes(_ fromIndex: Int, _ targetIndex: Int, _ before: Bool) {
        panes = moveStackBeforeOrAfter(panes, fromIndex, targetIndex, before)
    }
    func deleteStackFromPanes(_ key: String) {
        panes = deleteStack(panes, key)
    }
    func addStackPane() {
        panes = addStack(panes)
    }
}
