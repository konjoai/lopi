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
                .task { model.start() }
        }
        .windowStyle(.titleBar)
        .windowToolbarStyle(.unified)

        MenuBarExtra {
            MenuBarView()
                .environment(model)
                .preferredColorScheme(.dark)
        } label: {
            // Icon reflects live activity: filled bolt while agents run.
            Image(systemName: model.stats.running > 0 ? "bolt.fill" : "bolt")
        }
        .menuBarExtraStyle(.window)

        Settings {
            SettingsView()
                .environment(model)
                .preferredColorScheme(.dark)
        }
    }
}
