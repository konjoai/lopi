import SwiftUI

/// The Forge — lopi's multi-agent cockpit, now unified with Loop Stacks (macOS
/// Loop Stacks). A resizable, auto-tiling grid of stack panes: each pane is a
/// composer + a stack of loop cards flowing down to the currently-executing loop
/// at the bottom, plus either the bare-pane run button (≤1 card) or the purple
/// stack control dock (2+ cards). A one-card pane reads like the pre-unify Forge
/// pane — adding a second card is what turns a pane into a real stack, exactly
/// matching web's Unify-2 model where a bare pane *is* the one-card case. This is
/// the only cockpit nav item — web has no separate Forge route anymore, and
/// there's no separate Stacks screen here either.
struct ForgeView: View {
    @Environment(AppModel.self) private var model

    private var store: StackStore { model.stackStore }
    private var engine: StackRunEngine { model.stackEngine }

    /// Repo dropdown options for the config popovers/drawers — server-discovered
    /// git repos labelled `owner/name` and grouped by owner, with a leading
    /// "auto" (no override) entry. The labelling, grouping and order rules are
    /// the same pure code web's `/stacks` runs (`Stacks/RepoMenu.swift`), pinned
    /// to one shared golden fixture.
    private var repoChoices: [StackOption] { repoOptions(model.repos) }

    var body: some View {
        grid
            .background(Konjo.bg)
            .safeAreaInset(edge: .top, spacing: 0) { topBar }
            .task { await model.refreshRepos() }
    }

    private var topBar: some View {
        HStack(spacing: 12) {
            Text("lopi").font(Konjo.sans(15, weight: .bold)).foregroundStyle(Konjo.fg)
            ConnectionLED(state: model.connection)
            Spacer()
            Button { removePane() } label: {
                Image(systemName: "minus").font(.system(size: 17, weight: .semibold)).foregroundStyle(Konjo.fgDim)
            }
            .buttonStyle(.plain).focusEffectDisabled()
            .help("Remove stack").disabled(store.panes.count <= 1)
            Text("\(store.panes.count)").font(Konjo.mono(11)).foregroundStyle(Konjo.fgDim).monospacedDigit()
            Button { store.addStackPane() } label: {
                Image(systemName: "plus").font(.system(size: 20, weight: .semibold)).foregroundStyle(Konjo.ice)
            }
            .buttonStyle(.plain).focusEffectDisabled()
            .help("Add stack").disabled(store.panes.count >= 12)
        }
        .padding(.horizontal, 16).padding(.vertical, 8)
        .background(Konjo.bg)
        .overlay(Rectangle().fill(Konjo.line).frame(height: 1), alignment: .bottom)
    }

    private var grid: some View {
        PaneGridView(count: store.panes.count) { idx in
            if store.panes.indices.contains(idx) {
                let pane = store.panes[idx]
                StackPaneView(
                    store: store, engine: engine, pane: pane, index: idx, repoOptions: repoChoices,
                    onClose: store.panes.count > 1 ? { closePane(pane.key) } : nil)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .overlay(alignment: .top) { forgeBanner }
    }

    /// Honest connection truth over the grid: the stacks are client-only, but a
    /// live run needs the backend, so an unreachable server says so.
    @ViewBuilder private var forgeBanner: some View {
        if model.connection != .live {
            banner(
                title: "backend offline",
                detail: model.connection == .connecting ? "connecting to lopi sail…" : "start `lopi sail` to run stacks live",
                tint: Konjo.rose)
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

    private func closePane(_ key: String) {
        engine.clearRun(key)
        store.deleteStackFromPanes(key)
    }

    private func removePane() {
        guard store.panes.count > 1, let last = store.panes.last else { return }
        closePane(last.key)
    }
}
