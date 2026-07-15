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
public struct RepoEntry: Codable, Hashable {
    /// Absolute path — the value a launch actually uses.
    public var path: String
    public var owner: String?
    public var name: String

    public init(path: String, owner: String?, name: String) {
        self.path = path
        self.owner = owner
        self.name = name
    }
}

/// Section for repos with no GitHub identity. The space makes it uncollidable
/// with a real owner — a GitHub login cannot contain one.
public let NO_REMOTE_GROUP = "no github remote"

/// The no-override sentinel. Ungrouped, so `groupedMenu` pins it above every
/// section — but only while it matches the query, so that typing "lopi" doesn't
/// leave `auto` sitting at `flat[0]` where Return would select it.
public let AUTO_REPO_OPTION = StackOption(value: "", label: "auto")

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
public func repoOptions(_ repos: [RepoEntry]) -> [StackOption] {
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

/// One `@repo` autocomplete candidate — the full `@owner/name` token, ready to
/// splice into the goal text.
public struct RepoSuggestion: Equatable {
    public let token: String
    public let label: String
    public let hint: String
    /// The resolved run target — always the absolute path (`StackOption.value`),
    /// never the decorative label. `selectRepo` writes this straight onto
    /// `card.config.repo`, so the card's stored repo is a path from the moment
    /// it's picked, not re-derived later by re-parsing the label back out of
    /// free text (see `parseComposerInput`'s repo-resolution doc comment).
    public let value: String

    public init(token: String, label: String, hint: String, value: String) {
        self.token = token
        self.label = label
        self.hint = hint
        self.value = value
    }
}

/// Filtered repo suggestions for the goal field's `@` autocomplete, given its
/// *entire current value*. Only suggests while the *trailing* word in the
/// goal text is a bare `@token` — matches the grammar's `:alias "goal" @repo
/// ×N` order, where `@repo` is typically typed right after the goal text, so
/// (unlike the leading `:alias` token) this never needs the cursor position:
/// the match is always the end of the string, so "replace the match" and
/// "replace the string's tail" are the same operation. Reuses `optionMatches`
/// (label or hint, case-insensitive substring) so `@lopi` finds
/// `konjoai/lopi` the same way the repo dropdown's own search box would. The
/// `auto` sentinel (empty value) is never suggested. Mirrors the web
/// `repoAutocomplete` verbatim.
public func repoAutocomplete(_ goalText: String, _ repoOptions: [StackOption]) -> [RepoSuggestion] {
    guard let atIndex = goalText.lastIndex(of: "@") else { return [] }
    let isWordStart = atIndex == goalText.startIndex || goalText[goalText.index(before: atIndex)].isWhitespace
    guard isWordStart else { return [] }
    let after = goalText[goalText.index(after: atIndex)...]
    guard !after.contains(where: { $0.isWhitespace }) else { return [] }
    let query = after.lowercased()
    return repoOptions
        .filter { $0.value != "" && optionMatches($0, query) }
        .map { RepoSuggestion(token: "@\($0.label)", label: $0.label, hint: $0.hint, value: $0.value) }
}

/// Resolve an `@`-token's parsed label (e.g. `"konjoai/lopi"`, as recovered by
/// `parseComposerInput`'s `@(\S+)` grammar) back to its real path, by exact
/// label match against the fetched catalog. Returns the input unresolved when
/// no option matches — a stale/renamed repo, or free text typed by hand
/// outside the autocomplete flow — so a value is never silently dropped, only
/// left as a label `cardToTaskPayload` can't run (same as before this fix).
public func resolveRepoToken(_ label: String, _ repoOptions: [StackOption]) -> String {
    repoOptions.first(where: { $0.value != "" && $0.label == label })?.value ?? label
}

/// The inverse of `resolveRepoToken` — given a stored `config.repo` path, find
/// its display label for the provenance chip. Falls back to the basename of
/// the path (never the full absolute path, which is noisy UI) when the path
/// isn't in the current catalog — e.g. a repo that's since been removed from
/// disk.
public func repoLabelForPath(_ path: String, _ repoOptions: [StackOption]) -> String {
    if let found = repoOptions.first(where: { $0.value == path }) { return found.label }
    var trimmed = path
    while trimmed.hasSuffix("/") { trimmed.removeLast() }
    guard let slash = trimmed.lastIndex(of: "/") else { return trimmed.isEmpty ? path : trimmed }
    let base = String(trimmed[trimmed.index(after: slash)...])
    return base.isEmpty ? path : base
}
