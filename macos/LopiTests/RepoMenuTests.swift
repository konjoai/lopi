import XCTest
import LopiStacksKit
@testable import Lopi

/// Repo dropdown rule tests — the Swift port of
/// `web/src/lib/stores/repoMenu.test.ts`, same fixtures and assertions. Pure
/// functions only: no store, no mock, no timers.
///
/// The golden test decodes the SAME
/// `crates/lopi-ui/tests/fixtures/repo_menu_golden.json` that the web rule test
/// and the Rust shape test (`repo_identity.rs`) read. All three must agree —
/// that fixture is what stops the two surfaces drifting.
final class RepoMenuTests: XCTestCase {

    // MARK: OptionMenu — today's ungrouped catalogs are the degenerate case

    /// Model/effort/branch/autonomy carry no `group`. They must come back as one
    /// flat pinned list and behave exactly as before grouping existed.
    func testUngroupedCatalogIsOneFlatPinnedList() {
        let plain = [StackOption(value: "a", label: "Alpha"), StackOption(value: "b", label: "Beta")]
        let menu = groupedMenu(plain)

        XCTAssertTrue(menu.groups.isEmpty, "an ungrouped catalog produces no sections")
        XCTAssertEqual(menu.pinned.count, 2, "an ungrouped catalog pins every option")
        XCTAssertEqual(menu.flat.count, 2, "flat covers every row")
        XCTAssertEqual(menu.pinned.map(\.index), [0, 1], "indices are render order")
    }

    /// A grouped option appearing before an ungrouped one must not misindex the
    /// cursor: pinned rows render first regardless of source position.
    func testFlatIsIndexedInRenderOrderNotSourceOrder() {
        let mixed = [
            StackOption(value: "g", label: "Grouped", hint: "", group: "G"),
            StackOption(value: "p", label: "Pinned")
        ]
        let menu = groupedMenu(mixed)

        XCTAssertEqual(menu.flat.first?.value, "p", "pinned rows lead flat")
        XCTAssertEqual(menu.pinned.first?.index, 0, "the pinned row is cursor index 0")
        XCTAssertEqual(menu.groups.first?.rows.first?.index, 1, "grouped rows follow")
    }

    func testFilterMatchesLabelOrHintCaseInsensitively() {
        let hinted = [
            StackOption(value: "/h/lopi", label: "konjoai/lopi", hint: "/h/lopi", group: "konjoai"),
            StackOption(value: "/h/other", label: "konjoai/other", hint: "/h/other", group: "konjoai")
        ]
        XCTAssertEqual(groupedMenu(hinted, query: "LOPI").flat.count, 1, "query is case-insensitive")
        XCTAssertEqual(groupedMenu(hinted, query: "/h/other").flat.count, 1, "a path fragment matches via the hint")
        XCTAssertEqual(groupedMenu(hinted, query: "  ").flat.count, 2, "a whitespace-only query matches everything")
        XCTAssertTrue(groupedMenu(hinted, query: "nope").groups.isEmpty, "a section with no matches disappears")
    }

    // MARK: RepoMenu — the collision, the reason labels get a suffix

    /// A linked worktree and its main checkout report the same origin. They are
    /// two different run targets; they must not read as one row.
    func testAmbiguousLabelsAreDisambiguatedAndNeverMerged() {
        let collide = [
            RepoEntry(path: "/h/squish", owner: "konjoai", name: "squish"),
            RepoEntry(path: "/h/squish-wt", owner: "konjoai", name: "squish"),
            RepoEntry(path: "/h/lopi", owner: "konjoai", name: "lopi")
        ]
        let options = repoOptions(collide).filter { !$0.value.isEmpty }
        let labels = options.map(\.label)

        XCTAssertEqual(Set(labels).count, labels.count, "every label is unique")
        XCTAssertTrue(labels.contains("konjoai/squish · squish"), "an ambiguous label is suffixed with its directory")
        XCTAssertTrue(labels.contains("konjoai/squish · squish-wt"), "both sides of a collision are suffixed")
        XCTAssertTrue(labels.contains("konjoai/lopi"), "an UNambiguous label is left clean")
        XCTAssertEqual(
            options.map(\.value).sorted(),
            ["/h/lopi", "/h/squish", "/h/squish-wt"],
            "disambiguation never merges or drops a path"
        )
    }

    /// `CreateTaskRequest.repo` reaches git2::Repository::open and lopi never
    /// clones, so a label is decoration and the path is the fact.
    func testValuesAreAlwaysPaths() {
        let options = repoOptions([RepoEntry(path: "/h/lopi", owner: "konjoai", name: "lopi")])
            .filter { !$0.value.isEmpty }

        for opt in options {
            XCTAssertEqual(opt.hint, opt.value, "the hint is the path, which is what makes path-search work")
            XCTAssertTrue(opt.value.hasPrefix("/"), "the value stays an absolute path")
        }
        XCTAssertEqual(AUTO_REPO_OPTION.value, "", "auto is the empty no-override sentinel")
        XCTAssertNil(AUTO_REPO_OPTION.group, "auto is ungrouped, so it pins")
    }

    func testRepoWithNoGitHubIdentityKeepsItsPlace() {
        let options = repoOptions([RepoEntry(path: "/h/TinyStories", owner: nil, name: "TinyStories")])

        XCTAssertEqual(options[1].label, "TinyStories", "an unlabelled repo falls back to its name")
        XCTAssertEqual(options[1].group, NO_REMOTE_GROUP, "and lands in the junk drawer")
        XCTAssertEqual(options[1].value, "/h/TinyStories", "losing a label must never lose a repo")
    }

    /// If auto pinned unconditionally it would sit at flat[0] while you type
    /// "lopi", and Return would select auto instead of the row you're looking at.
    func testAutoStepsAsideUnderAQuery() {
        let options = repoOptions([RepoEntry(path: "/h/lopi", owner: "konjoai", name: "lopi")])

        XCTAssertEqual(groupedMenu(options, query: "").flat.first?.value, "", "auto leads an unfiltered menu")
        XCTAssertEqual(groupedMenu(options, query: "lopi").flat.first?.value, "/h/lopi", "auto steps aside under a query")
        XCTAssertTrue(groupedMenu(options, query: "lopi").pinned.isEmpty, "auto is gone when it does not match")
    }

    // MARK: `@repo` autocomplete — only the goal's trailing bare @token

    func testRepoAutocomplete() {
        let repos = repoOptions([
            RepoEntry(path: "/h/lopi", owner: "konjoai", name: "lopi"),
            RepoEntry(path: "/h/other", owner: "konjoai", name: "other-repo"),
            RepoEntry(path: "/h/TinyStories", owner: nil, name: "TinyStories")
        ])

        XCTAssertEqual(repoAutocomplete("fix the bug @lo", repos).count, 1, "a unique trailing @ prefix returns one match")
        XCTAssertEqual(repoAutocomplete("fix the bug @lo", repos).first?.token, "@konjoai/lopi", "the match carries the full @owner/name token")
        XCTAssertEqual(repoAutocomplete("fix the bug @lo", repos).first?.label, "konjoai/lopi", "the match carries the label")
        XCTAssertEqual(repoAutocomplete("@lo", repos).first?.token, "@konjoai/lopi", "an @ token works with no goal text before it too")
        XCTAssertEqual(repoAutocomplete("fix @konjoai", repos).count, 2, "matching is over the whole owner/name label, not just the name")
        XCTAssertEqual(repoAutocomplete("fix @tinystories", repos).first?.token, "@TinyStories", "a repo with no owner still autocompletes by name")
        XCTAssertEqual(repoAutocomplete("fix @nope", repos).count, 0, "no repo starts with an unknown prefix")
        XCTAssertEqual(repoAutocomplete("fix @auto", repos).count, 0, "the auto sentinel is never suggested")
        XCTAssertEqual(repoAutocomplete("fix the bug", repos).count, 0, "no trailing @ means no suggestions")
        XCTAssertEqual(repoAutocomplete("fix @lopi and more", repos).count, 0, "once a space follows the @token, the goal has moved on")
        XCTAssertEqual(repoAutocomplete("fix @lopi ", repos).count, 0, "a trailing space after a completed @token also closes the list")
        XCTAssertEqual(repoAutocomplete(":implement @lo", repos).first?.token, "@konjoai/lopi", "works alongside a leading :alias token too")
        XCTAssertEqual(repoAutocomplete("@lo", repos).first?.value, "/h/lopi", "the suggestion carries the resolved path, not just the label")
        // Regression: with `Foundation` imported, `String.contains("")` returns
        // `false` (NSString `range(of:)` semantics displace the stdlib's own
        // `Collection.contains`, which treats an empty needle as always-
        // contained) — `optionMatches` must special-case an empty query
        // explicitly, or a bare `@` with nothing typed yet silently shows no
        // suggestions instead of the full catalog.
        XCTAssertEqual(repoAutocomplete("@", repos).count, repos.count - 1, "a bare @ with no query yet suggests every real repo (catalog minus the auto sentinel)")
    }

    // MARK: `resolveRepoToken`/`repoLabelForPath` — the label↔path bridge
    //
    // The bug this closes: `@repo`'s inline grammar only ever recovers a
    // LABEL from free text (`parseComposerInput`'s `@`-token grammar), but
    // `config.repo` must hold the PATH `CreateTaskRequest.repo` actually
    // resolves against. These two helpers are the only place that
    // conversion happens.

    func testResolveRepoTokenAndLabelForPath() {
        let repos = repoOptions([
            RepoEntry(path: "/h/lopi", owner: "konjoai", name: "lopi"),
            RepoEntry(path: "/h/squish", owner: "konjoai", name: "squish")
        ])

        XCTAssertEqual(resolveRepoToken("konjoai/lopi", repos), "/h/lopi", "a known label resolves to its real path")
        XCTAssertEqual(resolveRepoToken("nonexistent/repo", repos), "nonexistent/repo", "an unresolvable label is left as-is, never dropped")
        XCTAssertEqual(repoLabelForPath("/h/lopi", repos), "konjoai/lopi", "a known path resolves back to its label")
        XCTAssertEqual(repoLabelForPath("/h/unknown/place", repos), "place", "an unresolvable path falls back to its basename, not the full path")
        XCTAssertEqual(repoLabelForPath("", repos), "auto", "an empty path matches the catalog's own AUTO_REPO_OPTION")
    }

    // MARK: The golden fixture — the cross-surface parity gate

    private struct Golden: Decodable {
        struct Row: Decodable {
            let value: String
            let label: String
            let hint: String?
            let group: String?
        }
        let repos: [RepoEntry]
        let options: [Row]
        let filtered: [String: [String]]
    }

    /// Resolved from this source file's path so there is a single source of
    /// truth (no copied fixture) — same approach as `AgentEventGoldenTests`.
    private func golden() throws -> Golden {
        let repoRoot = URL(fileURLWithPath: #file)
            .deletingLastPathComponent() // LopiTests/
            .deletingLastPathComponent() // macos/
            .deletingLastPathComponent() // repo root
        let url = repoRoot.appendingPathComponent("crates/lopi-ui/tests/fixtures/repo_menu_golden.json")
        return try JSONDecoder().decode(Golden.self, from: try Data(contentsOf: url))
    }

    func testGoldenFixtureProducesTheSameOptionsAsWeb() throws {
        let g = try golden()
        let built = repoOptions(g.repos)

        XCTAssertEqual(built.count, g.options.count, "golden: option count")
        for (i, want) in g.options.enumerated() {
            let got = built[i]
            XCTAssertEqual(got.value, want.value, "golden[\(i)] value")
            XCTAssertEqual(got.label, want.label, "golden[\(i)] label — \(want.label)")
            // Swift defaults `hint` to "" where TypeScript leaves it undefined;
            // the fixture spells that absence as null. Same meaning, two idioms.
            XCTAssertEqual(got.hint.isEmpty ? nil : got.hint, want.hint, "golden[\(i)] hint")
            XCTAssertEqual(got.group, want.group, "golden[\(i)] group")
        }
    }

    func testGoldenFixtureFiltersTheSameWayAsWeb() throws {
        let g = try golden()
        let built = repoOptions(g.repos)

        for (query, want) in g.filtered {
            let got = groupedMenu(built, query: query).flat.map(\.label)
            XCTAssertEqual(got, want, "golden: filter \(query.debugDescription)")
        }
    }
}
