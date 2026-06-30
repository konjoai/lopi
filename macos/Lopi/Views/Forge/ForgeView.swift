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
        .overlay(alignment: .top) { forgeBanner }
    }

    /// Honest connection truth over the grid: no synthetic agents are ever
    /// seeded, so an unreachable backend says so and an idle one shows empty.
    @ViewBuilder private var forgeBanner: some View {
        if model.connection != .live {
            banner(
                title: "backend offline",
                detail: model.connection == .connecting
                    ? "connecting to lopi sail…" : "start `lopi sail` to see live agents",
                tint: Konjo.rose
            )
        } else if model.liveAgents.isEmpty {
            banner(
                title: "no live sessions",
                detail: "launch a run with `lopi run` to populate the forge",
                tint: Konjo.fgMute
            )
        }
    }

    private func banner(title: String, detail: String, tint: Color) -> some View {
        VStack(spacing: 3) {
            Text(title).font(Konjo.sans(13, weight: .semibold)).foregroundStyle(tint)
            Text(detail).font(Konjo.mono(10)).foregroundStyle(Konjo.fgMute)
        }
        .padding(.horizontal, 18).padding(.vertical, 10)
        .background(Konjo.bg.opacity(0.82))
        .overlay(RoundedRectangle(cornerRadius: 10).stroke(Konjo.line2))
        .clipShape(RoundedRectangle(cornerRadius: 10))
        .padding(.top, 8)
        .allowsHitTesting(false)
    }

    private func agent(at idx: Int) -> LiveAgent? {
        guard layout.slots.indices.contains(idx), let id = layout.slots[idx] else { return nil }
        return model.liveAgents[id]
    }
}
