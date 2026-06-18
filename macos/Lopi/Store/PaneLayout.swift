import Foundation
import Observation

/// Pane/session layout for the macOS Forge — the native counterpart of the
/// web `layout` store, and the same fix for "deleted sessions reappear".
///
/// A **session** is a task the server knows about (lives in `AppModel.liveAgents`,
/// re-seeded from the snapshot on every connect). A **pane** is a grid slot that
/// happens to show one. Closing a pane must not delete the session, and deleting
/// a session must survive a reconnect — so three persisted sets are tracked:
///
///   - `slots`   — the ordered grid (session id or `nil` per slot).
///   - `closed`  — sessions parked in the sidebar; never auto-reopened.
///   - `deleted` — tombstones: permanently removed; filtered from every
///                 reconcile so a returning snapshot row cannot resurrect them.
///   - `known`   — every id ever seen, so a genuinely new task is told apart
///                 from a returning one and auto-placed into a free pane.
@Observable
@MainActor
final class PaneLayout {
    /// Default pane count — four concurrent agents, a 2×2 grid.
    static let defaultCount = 4
    static let maxPanes = 12
    static let minPanes = 1

    private(set) var slots: [String?]
    private(set) var closed: Set<String> = []
    private(set) var deleted: Set<String> = []
    private(set) var known: Set<String> = []

    @ObservationIgnored private let defaults: UserDefaults

    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
        self.slots = Self.loadSlots(defaults) ?? Array(repeating: nil, count: Self.defaultCount)
        self.closed = Self.loadSet(defaults, "lopi.closed")
        self.deleted = Self.loadSet(defaults, "lopi.deleted")
        self.known = Self.loadSet(defaults, "lopi.known")
    }

    // MARK: Tiling

    /// Rows/cols for `n` panes: 2 = halves, 3 = thirds, 4 = quarters, then 3-wide.
    static func dims(_ n: Int) -> (cols: Int, rows: Int) {
        switch n {
        case ..<2: return (1, 1)
        case 2: return (2, 1)
        case 3: return (3, 1)
        case 4: return (2, 2)
        case 5...6: return (3, 2)
        case 7...9: return (3, 3)
        default: return (4, Int(ceil(Double(n) / 4.0)))
        }
    }

    /// True when `id` was permanently deleted and must never be re-hydrated.
    func isDeleted(_ id: String) -> Bool { deleted.contains(id) }

    func agentIsOpen(_ id: String) -> Bool { slots.contains(id) }
    func isParked(_ id: String) -> Bool { closed.contains(id) && !slots.contains(id) }

    // MARK: Pane mutations

    /// Mount a session into the first free slot (appending if the grid is full).
    func openSession(_ id: String) {
        closed.remove(id)
        guard !slots.contains(id) else { persist(); return }
        if let empty = slots.firstIndex(where: { $0 == nil }) {
            slots[empty] = id
        } else if slots.count < Self.maxPanes {
            slots.append(id)
        } else {
            slots[slots.count - 1] = id
        }
        persist()
    }

    /// Close a pane: empties the slot and parks the session in the sidebar.
    /// The session itself is untouched (no server DELETE).
    func closePane(_ index: Int) {
        guard slots.indices.contains(index) else { return }
        if let id = slots[index] { closed.insert(id) }
        slots[index] = nil
        persist()
    }

    /// Permanently delete a session: tombstone it and drop it everywhere. The
    /// tombstone — not the server round-trip — is what keeps it gone.
    func tombstone(_ id: String) {
        deleted.insert(id)
        closed.remove(id)
        for i in slots.indices where slots[i] == id { slots[i] = nil }
        persist()
    }

    /// Grow or shrink the grid to exactly `n` panes (clamped).
    func setCount(_ n: Int) {
        let target = min(Self.maxPanes, max(Self.minPanes, n))
        if slots.count < target {
            slots.append(contentsOf: Array(repeating: nil, count: target - slots.count))
        } else if slots.count > target {
            slots = Array(slots.prefix(target))
        }
        persist()
    }

    func addPane() { setCount(slots.count + 1) }
    func removePane() { setCount(slots.count - 1) }

    /// Swap two slots — drives drag-to-reorder.
    func swap(_ a: Int, _ b: Int) {
        guard a != b, slots.indices.contains(a), slots.indices.contains(b) else { return }
        slots.swapAt(a, b)
        persist()
    }

    /// Auto-place genuinely new sessions; leave known/closed ones where they are.
    /// Returns the ids that were auto-placed.
    @discardableResult
    func reconcile(_ ids: some Sequence<String>) -> [String] {
        let fresh = ids.filter { !known.contains($0) && !deleted.contains($0) }
        guard !fresh.isEmpty else { return [] }
        var placed: [String] = []
        for id in fresh {
            known.insert(id)
            guard !closed.contains(id), !slots.contains(id) else { continue }
            if let empty = slots.firstIndex(where: { $0 == nil }) {
                slots[empty] = id
                placed.append(id)
            }
        }
        persist()
        return placed
    }

    // MARK: Persistence

    private func persist() {
        if let data = try? JSONEncoder().encode(slots) {
            defaults.set(data, forKey: "lopi.slots")
        }
        defaults.set(Array(closed), forKey: "lopi.closed")
        defaults.set(Array(deleted), forKey: "lopi.deleted")
        defaults.set(Array(known), forKey: "lopi.known")
    }

    private static func loadSlots(_ d: UserDefaults) -> [String?]? {
        guard let data = d.data(forKey: "lopi.slots") else { return nil }
        return try? JSONDecoder().decode([String?].self, from: data)
    }

    private static func loadSet(_ d: UserDefaults, _ key: String) -> Set<String> {
        Set((d.array(forKey: key) as? [String]) ?? [])
    }
}
