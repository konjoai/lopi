import XCTest

/// Stack-Chain-1 / Popover-Fix-1 — first XCUITest coverage in this repo.
/// Drives the real built `Lopi.app` via the accessibility-based XCUITest
/// harness (not interactive computer-use screen control — see
/// `LopiUITests`'s target comment in `project.yml`), talking to whatever
/// backend the app is configured for (defaults to `127.0.0.1:3000`, same as
/// a developer's local `lopi sail`).
///
/// Backend correctness (the chain actually persists, restart-resume, the
/// popover repositions) already has direct coverage — this suite exercises
/// the native UI surface on top of it: element identifiers added alongside
/// this sprint (`stack.dockExpand`, `"Schedule the entire stack"` via
/// `CardbarButton`'s `.accessibilityIdentifier(help)`, `stack.scheduleToggle`,
/// `stack.goalField`) make these queries exact rather than guesswork.
final class StackChainScheduleUITests: XCTestCase {
    var app: XCUIApplication!

    override func setUpWithError() throws {
        continueAfterFailure = false
        app = XCUIApplication()
        app.launch()
    }

    override func tearDownWithError() throws {
        app.terminate()
    }

    /// Add two cards to the first stack pane, then open the stack control
    /// dock's schedule popover and confirm the toggle and cron builder
    /// render — the UI-level half of Stack-Chain-1 (the actual
    /// `/api/schedule-chains` submission is covered by
    /// `crates/lopi-ui/src/web/schedule_chains_tests.rs`'s Rust-side
    /// equivalent handlers and `AppModel.syncStackSchedule`'s callers).
    func testScheduleStackPopoverOpensWithToggleAndCronBuilder() throws {
        let goalField = app.textFields["stack.goalField"].firstMatch
        XCTAssertTrue(goalField.waitForExistence(timeout: 10), "goal field should render on the Loop Stack screen")

        goalField.click()
        goalField.typeText("xcuitest verify card one")
        app.buttons["add to stack"].firstMatch.click()

        let dockExpand = app.buttons["stack.dockExpand"].firstMatch
        XCTAssertTrue(dockExpand.waitForExistence(timeout: 5), "dock should appear once a card is queued")
        dockExpand.click()

        let scheduleButton = app.buttons["Schedule the entire stack"].firstMatch
        XCTAssertTrue(scheduleButton.waitForExistence(timeout: 5))
        scheduleButton.click()

        let toggle = app.buttons["stack.scheduleToggle"].firstMatch
        XCTAssertTrue(toggle.waitForExistence(timeout: 5), "schedule popover should open with its toggle visible")
        toggle.click()

        // Toggling on mounts the cron builder — the same content-growth
        // moment that overflowed on web pre-fix (KT2).
        XCTAssertTrue(
            app.staticTexts["next runs:"].waitForExistence(timeout: 5),
            "cron builder should render once the schedule toggle is on"
        )
    }

    /// The macOS analogue of `web/e2e/popover-visibility.spec.ts`: at a
    /// short window height, the schedule popover (after the toggle grows its
    /// content) must stay fully within the window's frame. Unlike the web
    /// fix, no `arrowEdge` values were changed this session (KT3 blocked —
    /// see `docs/ops/PARITY_AUDIT_2026-07-16.md` §2) — this test is what
    /// determines, the next time it's run with a live app, whether macOS
    /// ever needed the fix at all.
    func testScheduleStackPopoverStaysOnScreenAtShortWindowHeight() throws {
        let window = app.windows.firstMatch
        XCTAssertTrue(window.waitForExistence(timeout: 10))
        window.resize(to: CGSize(width: 1200, height: 700))

        let goalField = app.textFields["stack.goalField"].firstMatch
        XCTAssertTrue(goalField.waitForExistence(timeout: 10))
        goalField.click()
        goalField.typeText("xcuitest short viewport card")
        app.buttons["add to stack"].firstMatch.click()

        app.buttons["stack.dockExpand"].firstMatch.click()
        app.buttons["Schedule the entire stack"].firstMatch.click()

        let toggle = app.buttons["stack.scheduleToggle"].firstMatch
        XCTAssertTrue(toggle.waitForExistence(timeout: 5))
        toggle.click()
        XCTAssertTrue(app.staticTexts["next runs:"].waitForExistence(timeout: 5))

        let popover = app.popovers.firstMatch
        XCTAssertTrue(popover.waitForExistence(timeout: 5), "schedule popover should be queryable as a popover element")
        let windowFrame = window.frame
        let popoverFrame = popover.frame
        XCTAssertGreaterThanOrEqual(popoverFrame.minY, windowFrame.minY, "popover top must not clip above the window")
        XCTAssertLessThanOrEqual(
            popoverFrame.maxY, windowFrame.maxY,
            "popover bottom must not clip below the window — this is the exact bug KT2 found on web"
        )
    }
}

private extension XCUIElement {
    /// `XCUIApplication` windows don't expose a direct resize API; drag the
    /// bottom-right resize handle instead. Best-effort — some window styles
    /// don't expose a draggable corner, in which case this is a no-op and
    /// the test runs at whatever size the window opened at.
    func resize(to size: CGSize) {
        guard exists else { return }
        let start = coordinate(withNormalizedOffset: CGVector(dx: 1.0, dy: 1.0))
        let target = frame.origin.applying(.init(translationX: size.width, y: size.height))
        let end = coordinate(withNormalizedOffset: CGVector(dx: 0, dy: 0))
            .withOffset(CGVector(dx: target.x - frame.origin.x, dy: target.y - frame.origin.y))
        start.press(forDuration: 0.05, thenDragTo: end)
    }
}
