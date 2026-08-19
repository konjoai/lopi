import XCTest

/// Regression test for a real race: `RootTabView`'s tab-bar "+" button used
/// to drive `.fullScreenCover(isPresented:)` off a separate `Bool` + a
/// `String?` key, and the cover could present before the key propagated to
/// the content closure — a blank black screen, confirmed live (not just in
/// theory) before the fix moved to `.fullScreenCover(item:)`. This asserts
/// the composer actually renders after the tap, not just that the app
/// doesn't crash.
final class NewStackFlowUITests: XCTestCase {
    func testTapNewStackShowsComposer() throws {
        let app = XCUIApplication()
        app.launch()

        let plus = app.buttons["tabbar.newStack"]
        XCTAssertTrue(plus.waitForExistence(timeout: 5), "new-stack button never appeared")
        plus.tap()

        XCTAssertTrue(
            app.staticTexts["NEW PROMPT"].waitForExistence(timeout: 3),
            "composer's \"new prompt\" tag never appeared after tapping + — the fullScreenCover likely presented blank"
        )
    }
}
