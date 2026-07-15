import Foundation

// Pane-keyed dispatch + stack-level (whole-pane) ops — the pure port of the
// `applyToPaneCards` / `insertIntoPane` / `duplicateStack` / `reorderStacks` /
// `deleteStack` / `paneIsBare` / `makeBlankStack` / `addStack` block in
// `stores/stack.ts`. Pure and isolated per pane. Foundation only.

/// True when a pane should render as a *bare* box — composer + an idle orb,
/// no connector, no purple dock. Only an empty pane is bare; a pane earns its
/// full stack chrome (dock + connectors) as soon as it holds its first card,
/// so the run/schedule/guardrails/goal controls are visible from the very
/// first prompt.
public func paneIsBare(_ pane: StackPaneState) -> Bool {
    pane.cards.isEmpty
}

/// A fresh, empty stack pane with its own config and a unique key.
public func makeBlankStack(_ title: String = "new stack") -> StackPaneState {
    StackPaneState(key: makeId(), title: title, cards: [], config: defaultStackConfig())
}

/// Append a fresh blank pane — the create-from-scratch path.
public func addStack(_ state: [StackPaneState]) -> [StackPaneState] {
    state + [makeBlankStack()]
}

/// Apply a pure card-list transform to one pane by key, leaving every other
/// pane untouched. No-op for an unknown key.
public func applyToPaneCards(_ state: [StackPaneState], _ key: String, _ fn: ([StackCard]) -> [StackCard]) -> [StackPaneState] {
    guard let idx = state.firstIndex(where: { $0.key == key }) else { return state }
    var next = state
    next[idx].cards = fn(next[idx].cards)
    return next
}

/// Insert a card into a specific pane at `index`.
public func insertIntoPane(_ state: [StackPaneState], _ key: String, _ index: Int, _ card: StackCard) -> [StackPaneState] {
    applyToPaneCards(state, key) { insertCardAt($0, index, card) }
}

/// Clone a whole stack — title, config, and every card — in place, immediately
/// after the original. Every cloned card gets a fresh id and wiped run state;
/// the clone gets a fresh key and its own config (value types make this
/// automatic). No-op if the key isn't present.
public func duplicateStack(_ state: [StackPaneState], _ key: String) -> [StackPaneState] {
    guard let idx = state.firstIndex(where: { $0.key == key }) else { return state }
    let original = state[idx]
    let clonedCards = original.cards.map { card -> StackCard in
        var c = card
        c.id = makeId()
        c.status = .idle
        c.iteration = nil
        c.taskId = nil
        return c
    }
    let clone = StackPaneState(
        key: makeId(),
        title: "\(original.title) copy",
        cards: clonedCards,
        config: original.config)
    var next = state
    next.insert(clone, at: idx + 1)
    return next
}

/// Copy another currently-open pane's cards into this one, replacing whatever
/// this pane already has — the "saved stacks" section of the stack-scope
/// templates menu (Stack-Templates-1 §5). Deliberately not a real stack
/// library: nothing persists beyond the two in-memory panes `StackStore`
/// already holds. Every copied card gets a fresh id and wiped run state,
/// mirroring `duplicateStack`'s per-card reset. No-op if either key is
/// missing or they're the same pane.
public func loadStackCardsInto(_ state: [StackPaneState], targetKey: String, sourceKey: String) -> [StackPaneState] {
    if targetKey == sourceKey { return state }
    guard let source = state.first(where: { $0.key == sourceKey }) else { return state }
    let copiedCards = source.cards.map { card -> StackCard in
        var c = card
        c.id = makeId()
        c.status = .idle
        c.iteration = nil
        c.taskId = nil
        return c
    }
    return applyToPaneCards(state, targetKey) { _ in copiedCards }
}

/// Move the stack at `from` to index `to`. Out-of-range is a no-op.
public func reorderStacks(_ state: [StackPaneState], _ from: Int, _ to: Int) -> [StackPaneState] {
    guard from >= 0, from < state.count, to >= 0, to < state.count else { return state }
    var next = state
    let moved = next.remove(at: from)
    next.insert(moved, at: to)
    return next
}

/// Drag-and-drop-friendly stack reorder — the pane-level twin of
/// `moveCardBeforeOrAfter`.
public func moveStackBeforeOrAfter(_ state: [StackPaneState], _ fromIndex: Int, _ targetIndex: Int, _ before: Bool) -> [StackPaneState] {
    if fromIndex == targetIndex { return state }
    let to = fromIndex < targetIndex
        ? (before ? targetIndex - 1 : targetIndex)
        : (before ? targetIndex : targetIndex + 1)
    return reorderStacks(state, fromIndex, to)
}

/// Drop a stack by key. Refuses to delete the last remaining pane (no
/// pane-creation affordance to recover — a deliberate floor).
public func deleteStack(_ state: [StackPaneState], _ key: String) -> [StackPaneState] {
    if state.count <= 1 { return state }
    return state.filter { $0.key != key }
}
