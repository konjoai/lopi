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
        // Custom black bar instead of system toolbar items, which carry an
        // unwanted grey "glass" well behind their content.
        .safeAreaInset(edge: .top, spacing: 0) { topBar }
        .onAppear { layout.reconcile(model.liveAgents.keys) }
        .onChange(of: model.liveAgents.keys.sorted()) { _, keys in
            layout.reconcile(keys)
        }
    }

    private var topBar: some View {
        HStack(spacing: 12) {
            Text("lopi").font(Konjo.sans(15, weight: .bold)).foregroundStyle(Konjo.fg)
            ConnectionLED(state: model.connection)
            Spacer()
            Button { layout.removePane() } label: {
                Image(systemName: "minus")
                    .font(.system(size: 17, weight: .semibold)).foregroundStyle(Konjo.fgDim)
            }
            .buttonStyle(.plain).focusEffectDisabled()
            .help("Remove pane").disabled(layout.slots.count <= PaneLayout.minPanes)
            Text("\(layout.slots.count)")
                .font(Konjo.mono(11)).foregroundStyle(Konjo.fgDim).monospacedDigit()
            Button { layout.addPane() } label: {
                Image(systemName: "plus")
                    .font(.system(size: 20, weight: .semibold)).foregroundStyle(Konjo.ice)
            }
            .buttonStyle(.plain).focusEffectDisabled()
            .help("Add pane").disabled(layout.slots.count >= PaneLayout.maxPanes)
        }
        .padding(.horizontal, 16).padding(.vertical, 8)
        .background(Konjo.bg)
        .overlay(Rectangle().fill(Konjo.line).frame(height: 1), alignment: .bottom)
    }

    private var grid: some View {
        PaneGridView(count: layout.slots.count) { idx in
            AgentPaneView(
                agent: agent(at: idx),
                controls: controls,
                paneCount: layout.slots.count,
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
