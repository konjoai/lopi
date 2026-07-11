import SwiftUI

@main
struct LopiApp: App {
    @State private var model = AppModel()

    var body: some Scene {
        WindowGroup(id: "main") {
            RootView()
                .environment(model)
                .frame(minWidth: 900, minHeight: 600)
                .preferredColorScheme(.dark)
                // Tint every system control (segmented pickers, default
                // buttons, toggles, focus) with the Konjo accent so nothing
                // falls back to system blue.
                .tint(Konjo.ice)
                .task { model.start() }
        }
        // Hidden title bar so the app's own black top bar (lopi · live · panes)
        // is the single top row — no separate system title band, no grey wells.
        .windowStyle(.hiddenTitleBar)

        MenuBarExtra {
            MenuBarView()
                .environment(model)
                .preferredColorScheme(.dark)
                .tint(Konjo.ice)
        } label: {
            // Icon reflects live activity: filled bolt while agents run.
            Image(systemName: model.runningCount > 0 ? "bolt.fill" : "bolt")
        }
        .menuBarExtraStyle(.window)

        Settings {
            SettingsView()
                .environment(model)
                .preferredColorScheme(.dark)
                .tint(Konjo.ice)
        }
    }
}
