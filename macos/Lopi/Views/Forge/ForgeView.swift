import SwiftUI

/// The Forge — lopi's multi-agent cockpit. A sessions rail beside a resizable,
/// auto-tiling grid of agent panes (default four), each with a live orb and a
/// launcher. Mirrors the web Forge, including the close-pane ≠ delete-session
/// model that keeps deleted sessions from resurrecting on reconnect.
struct ForgeView: View {
    @Environment(AppModel.self) private var model
    /// Shared with the unified sidebar in RootView (sessions ↔ panes).
    var layout: PaneLayout
    @State private var controls = LaunchControls()

    var body: some View {
        grid
        .background(Konjo.bg)
        .toolbar {
            ToolbarItemGroup(placement: .automatic) {
                Button { layout.removePane() } label: { Image(systemName: "rectangle.split.2x1") }
                    .help("Remove pane")
                    .disabled(layout.slots.count <= PaneLayout.minPanes)
                Text("\(layout.slots.count)")
                    .font(Konjo.mono(11)).foregroundStyle(Konjo.fgDim).monospacedDigit()
                Button { layout.addPane() } label: { Image(systemName: "plus.rectangle") }
                    .help("Add pane")
                    .disabled(layout.slots.count >= PaneLayout.maxPanes)
            }
        }
        .onAppear { layout.reconcile(model.liveAgents.keys) }
        .onChange(of: model.liveAgents.keys.sorted()) { _, keys in
            layout.reconcile(keys)
        }
    }

    private var grid: some View {
        PaneGridView(count: layout.slots.count) { idx in
            AgentPaneView(
                agent: agent(at: idx),
                controls: controls,
                onClose: { layout.closePane(idx) }
            )
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private func agent(at idx: Int) -> LiveAgent? {
        guard layout.slots.indices.contains(idx), let id = layout.slots[idx] else { return nil }
        return model.liveAgents[id]
    }
}
