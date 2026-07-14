import Foundation

// Repo dropdown policy: how a `/api/repos` entry becomes a labelled, grouped,
// ordered `StackOption`. Pure — `OptionMenu.swift` does the grouping and decides
// nothing about order; every decision below is made here, once.
//
// The 1:1 port of web's `stores/repoMenu.ts`. Both are pinned to one shared
// golden fixture (`crates/lopi-ui/tests/fixtures/repo_menu_golden.json`) so the
// two surfaces cannot drift — the same mechanism `AgentEventGoldenTests.swift`
// and `parser.test.ts` use for `agent_event_golden.json`.

/// A repo as `GET /api/repos` reports it. `owner` is nil when the checkout has
/// no origin remote, or its origin is not GitHub.
struct RepoEntry: Codable, Hashable {
    /// Absolute path — the value a launch actually uses.
    var path: String
    var owner: String?
    var name: String
}

/// Section for repos with no GitHub identity. The space makes it uncollidable
/// with a real owner — a GitHub login cannot contain one.
let NO_REMOTE_GROUP = "no github remote"

/// The no-override sentinel. Ungrouped, so `groupedMenu` pins it above every
/// section — but only while it matches the query, so that typing "lopi" doesn't
/// leave `auto` sitting at `flat[0]` where Return would select it.
let AUTO_REPO_OPTION = StackOption(value: "", label: "auto")

/// The trailing path segment — the disambiguator when one `owner/name` covers
/// two checkouts.
private func basename(_ path: String) -> String {
    var trimmed = path
    while trimmed.hasSuffix("/") { trimmed.removeLast() }
    guard let slash = trimmed.lastIndex(of: "/") else { return trimmed }
    return String(trimmed[trimmed.index(after: slash)...])
}

/// `owner/name`, or the bare name for a repo with no GitHub identity.
private func baseLabel(_ r: RepoEntry) -> String {
    guard let owner = r.owner, !owner.isEmpty else { return r.name }
    return "\(owner)/\(r.name)"
}

private func groupOf(_ r: RepoEntry) -> String { r.owner ?? NO_REMOTE_GROUP }

/// Three-way compare over Unicode scalars — deliberately *not* Swift's `<`.
///
/// Swift's default `String` ordering is not the UTF-16 code-unit ordering JS's
/// `<` uses, so a label carrying a non-ASCII character — the `·` an ambiguous
/// label gets — could sort differently on the two surfaces. Scalar order matches
/// JS for every character in the BMP, which is the whole alphabet of a repo
/// label. The golden fixture would catch a divergence; this stops one existing.
private func scalarCompare(_ a: String, _ b: String) -> Int {
    if a.unicodeScalars.lexicographicallyPrecedes(b.unicodeScalars) { return -1 }
    if b.unicodeScalars.lexicographicallyPrecedes(a.unicodeScalars) { return 1 }
    return 0
}

private func tally(_ keys: [String]) -> [String: Int] {
    keys.reduce(into: [:]) { counts, k in counts[k, default: 0] += 1 }
}

/// Order two sections: junk drawer last, then most checkouts first, then name.
private func compareGroups(_ ga: String, _ gb: String, _ counts: [String: Int]) -> Int {
    // 1. The junk drawer sinks, however big it grows.
    let junkA = ga == NO_REMOTE_GROUP ? 1 : 0
    let junkB = gb == NO_REMOTE_GROUP ? 1 : 0
    if junkA != junkB { return junkA - junkB }
    // 2. Then the owners you have most checkouts of.
    let countA = counts[ga] ?? 0
    let countB = counts[gb] ?? 0
    if countA != countB { return countB - countA }
    // 3. Case-INSENSITIVE, else a capital's ASCII value puts `SteveFeldman`
    //    ahead of `bmaltais`. 4. then case-sensitive, to break exact ties.
    let ci = scalarCompare(ga.lowercased(), gb.lowercased())
    return ci != 0 ? ci : scalarCompare(ga, gb)
}

/// Build the repo dropdown's options: `auto`, then every repo labelled
/// `owner/name`, grouped by owner, sorted so the sections a user works in most
/// come first.
///
/// The value is always the absolute path — a path is the only thing that
/// identifies a run target (`CreateTaskRequest.repo` reaches
/// `git2::Repository::open`, and lopi never clones), so the label is decoration
/// and the path is the fact.
///
/// Two checkouts can share one `owner/name`: a linked worktree and its main repo
/// both report the origin they share. Path labels can't collide; `owner/name`
/// labels can, and two different run targets rendering as one row is exactly the
/// failure this must not introduce — so an ambiguous label, and only an ambiguous
/// one, is suffixed with its directory name.
func repoOptions(_ repos: [RepoEntry]) -> [StackOption] {
    let bases = tally(repos.map(baseLabel))
    let counts = tally(repos.map(groupOf))

    var options: [StackOption] = repos.map { r in
        let base = baseLabel(r)
        let ambiguous = (bases[base] ?? 0) > 1
        return StackOption(
            value: r.path,
            label: ambiguous ? "\(base) · \(basename(r.path))" : base,
            hint: r.path,
            group: groupOf(r)
        )
    }

    options.sort { a, b in
        let ga = a.group ?? ""
        let gb = b.group ?? ""
        if ga != gb { return compareGroups(ga, gb, counts) < 0 }
        // 5/6/7. Rows within a section. The chain ends on the path, which is
        //        unique — so the order is TOTAL, which is why JS's stable sort
        //        and Swift's unstable one produce the identical array. That's
        //        load-bearing, not decoration.
        let ci = scalarCompare(a.label.lowercased(), b.label.lowercased())
        if ci != 0 { return ci < 0 }
        let cs = scalarCompare(a.label, b.label)
        if cs != 0 { return cs < 0 }
        return scalarCompare(a.value, b.value) < 0
    }

    return [AUTO_REPO_OPTION] + options
}
