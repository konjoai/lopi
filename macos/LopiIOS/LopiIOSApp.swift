import SwiftUI

/// iOS entry point. Reuses the same `AppModel` the macOS app drives —
/// `Networking`/`Store`/`Theme`/`Stacks` are shared sources (see
/// `project.yml`'s `LopiIOS` target, none of them import AppKit).
///
/// No `MenuBarExtra`/`Settings` scene here: neither has an iOS analogue.
/// `.windowStyle(.hiddenTitleBar)` is also macOS-only and dropped — iOS has
/// no title bar to hide.
@main
struct LopiIOSApp: App {
    @State private var model = AppModel()

    var body: some Scene {
        WindowGroup {
            StackOverviewScreen()
                .environment(model)
                .preferredColorScheme(.dark)
                .tint(Konjo.ice)
                .task { model.start() }
        }
    }
}
