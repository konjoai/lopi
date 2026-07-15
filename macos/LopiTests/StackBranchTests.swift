import XCTest
import LopiStacksKit
@testable import Lopi

/// Branch-resolution tests — the Swift port of
/// `web/src/lib/stores/stackDefaults.test.ts`, same fixtures and assertions.
/// Pure function only: no store, no mock, no timers. The two surfaces must agree
/// on which branch a repo switch lands on.
final class StackBranchTests: XCTestCase {

    private let branches = ["main", "dev", "feat/x"]

    // An explicit, still-valid choice always survives a repo switch.
    func testValidExplicitBranchIsKept() {
        XCTAssertEqual(resolveBranch("feat/x", branches, "main"), "feat/x", "a valid explicit branch is kept")
        XCTAssertEqual(resolveBranch("main", branches, "main"), "main", "a branch equal to HEAD is kept")
    }

    // Unset or now-invalid falls back to the repo's HEAD.
    func testUnsetOrInvalidAdoptsHead() {
        XCTAssertEqual(resolveBranch("", branches, "dev"), "dev", "an unset branch adopts HEAD")
        XCTAssertEqual(resolveBranch("gone", branches, "dev"), "dev", "a branch absent from the new repo adopts HEAD")
    }

    // HEAD itself can be unusable — detached HEAD reports empty, and a stale
    // cache entry can name a branch the repo no longer has.
    func testUnusableHeadFallsBackToFirstBranch() {
        XCTAssertEqual(resolveBranch("", branches, ""), "main", "no HEAD falls back to the first branch")
        XCTAssertEqual(resolveBranch("gone", branches, "also-gone"), "main", "an invalid HEAD falls back to the first branch")
    }

    // An empty list means "no knowledge", not "wipe it". Unfetched or
    // fetch-failed must never clobber what the user set: `branch` reaches the
    // server as a planning constraint, so a silent wipe would relaunch against
    // the wrong target.
    func testUnknownRepoLeavesCurrentAlone() {
        XCTAssertEqual(resolveBranch("feat/x", [], ""), "feat/x", "an unknown repo leaves an explicit branch alone")
        XCTAssertEqual(resolveBranch("", [], ""), "", "an unknown repo leaves an unset branch unset")
    }

    // Convergence: re-resolving a resolved value is a no-op, so the `syncBranch`
    // write-back in the config views cannot loop.
    func testResolveIsIdempotent() {
        let once = resolveBranch("gone", branches, "dev")
        XCTAssertEqual(resolveBranch(once, branches, "dev"), once, "resolveBranch is idempotent")
    }

    // The cold-start seed is a real default, not a phantom.
    func testFreshStackSeedsFromSeedBranch() {
        XCTAssertEqual(DEFAULT_STACK_DEFAULTS.branch, SEED_BRANCH, "a fresh stack seeds from SEED_BRANCH")
    }
}
