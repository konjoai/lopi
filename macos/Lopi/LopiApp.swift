import SwiftUI

@main
struct LopiApp: App {
    @StateObject private var model = AppModel()

    var body: some Scene {
        WindowGroup(id: "main") {
            RootView()
                .environmentObject(model)
                .frame(minWidth: 900, minHeight: 600)
                .preferredColorScheme(.dark)
                .task { model.start() }
        }
        .windowStyle(.titleBar)
        .windowToolbarStyle(.unified)

        MenuBarExtra {
            MenuBarView()
                .environmentObject(model)
                .preferredColorScheme(.dark)
        } label: {
            // Icon reflects live activity: filled bolt while agents run.
            Image(systemName: model.stats.running > 0 ? "bolt.fill" : "bolt")
        }
        .menuBarExtraStyle(.window)

        Settings {
            SettingsView()
                .environmentObject(model)
                .preferredColorScheme(.dark)
        }
    }
}
