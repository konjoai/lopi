import Foundation

// Grouping + filtering for the config dropdowns' lists — pure, and deliberately
// policy-free. The 1:1 port of web's `stores/optionMenu.ts`.
//
// Order is *not* decided here: sections appear in the order their first option
// appears, and options keep their given order within a section. The caller sorts
// (`RepoMenu.swift`), which is what stops sections reshuffling under a live
// filter — an order recomputed from *matching* counts would jump on every
// keystroke.
//
// An option with no `group` pins above every section. So a catalog where nothing
// carries a `group` — every field but `repo` — comes back as one flat `pinned`
// list and behaves exactly as it did before grouping existed. Today's dropdown
// is the degenerate case of this function, not a branch around it.

/// An option plus its index into `OptionMenu.flat` — the index the keyboard
/// cursor uses. Precomputed so the view never does index arithmetic across two
/// nested loops.
struct MenuRow: Identifiable, Hashable {
    var opt: StackOption
    var index: Int

    var id: String { opt.value }
}

/// One section: the `group` key its options share, and its rows.
struct OptionGroup: Identifiable, Hashable {
    var key: String
    var rows: [MenuRow]

    var id: String { key }
}

/// A menu partitioned for rendering: pinned rows, ordered sections, and the flat
/// cursor list.
struct OptionMenu: Hashable {
    /// Ungrouped options, in their given order — rendered above every section.
    var pinned: [MenuRow]
    var groups: [OptionGroup]
    /// Every selectable row, in render order. Section headers are absent, so a
    /// cursor walking this list steps over them for free.
    var flat: [StackOption]
}

/// Does `opt` survive `q` (already trimmed and lowercased)? Case-insensitive
/// substring over the label and the hint. For repos the hint *is* the absolute
/// path, so this one predicate is "match `owner/name` or the path" — with no
/// second field to keep in sync across two languages.
private func optionMatches(_ opt: StackOption, _ q: String) -> Bool {
    opt.label.lowercased().contains(q) || opt.hint.lowercased().contains(q)
}

/// Partition `options` into pinned rows and ordered sections, keeping only what
/// matches `query`.
func groupedMenu(_ options: [StackOption], query: String = "") -> OptionMenu {
    let q = query.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
    let passing = q.isEmpty ? options : options.filter { optionMatches($0, q) }

    var pinnedOpts: [StackOption] = []
    // Swift's Dictionary is unordered, so first-appearance order is kept by hand
    // — web relies on JS Map insertion order for the same guarantee.
    var order: [String] = []
    var buckets: [String: [StackOption]] = [:]
    for opt in passing {
        guard let group = opt.group, !group.isEmpty else {
            pinnedOpts.append(opt)
            continue
        }
        if buckets[group] == nil { order.append(group) }
        buckets[group, default: []].append(opt)
    }

    // Index in render order (pinned, then each section) rather than source order
    // — the cursor walks what the user sees.
    var index = 0
    let pinned = pinnedOpts.map { opt -> MenuRow in
        defer { index += 1 }
        return MenuRow(opt: opt, index: index)
    }
    let groups = order.map { key -> OptionGroup in
        let rows = (buckets[key] ?? []).map { opt -> MenuRow in
            defer { index += 1 }
            return MenuRow(opt: opt, index: index)
        }
        return OptionGroup(key: key, rows: rows)
    }
    let flat = pinned.map(\.opt) + groups.flatMap { $0.rows.map(\.opt) }

    return OptionMenu(pinned: pinned, groups: groups, flat: flat)
}
