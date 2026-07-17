import SwiftUI
import LopiStacksKit

/// The Forge — lopi's multi-agent cockpit, now unified with Loop Stacks (macOS
/// Loop Stacks). A resizable, auto-tiling grid of stack panes: each pane is a
/// composer + a stack of loop cards flowing down to the currently-executing loop
/// at the bottom, plus the purple stack control dock — present from the very
/// first (empty) pane, not just once a card has been committed, so stack-level
/// defaults/schedule/guardrails/templates can be set up before writing any
/// prompt. This is the only cockpit nav item — web has no separate Forge route
/// anymore, and there's no separate Stacks screen here either.
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

    /// Per-pane rendered width, keyed by grid index — measured so the stack
    /// drop-destination can tell which half of the target pane the cursor is
    /// over (before/after), matching web's cursor-midpoint rule.
    @State private var paneWidths: [Int: CGFloat] = [:]

    private struct PaneWidthKey: PreferenceKey {
        static var defaultValue: [Int: CGFloat] = [:]
        static func reduce(value: inout [Int: CGFloat], nextValue: () -> [Int: CGFloat]) {
            value.merge(nextValue()) { _, new in new }
        }
    }

    var body: some View {
        grid
            .background(Konjo.bg)
            .safeAreaInset(edge: .top, spacing: 0) { topBar }
            .onPreferenceChange(PaneWidthKey.self) { paneWidths = $0 }
            .task { await model.refreshRepos() }
    }

    private var topBar: some View {
        HStack(spacing: 12) {
            LopiWordmark(fontSize: 15, weight: .bold)
            ConnectionLED(state: model.connection)
            Spacer()
            Button { removePane() } label: {
                Image(systemName: "minus").font(.system(size: 18, weight: .semibold)).foregroundStyle(Konjo.fgDim)
            }
            .buttonStyle(.plain).focusEffectDisabled()
            .help("Remove stack").disabled(store.panes.count <= 1)
            Text("\(store.panes.count)").font(Konjo.mono(11)).foregroundStyle(Konjo.fgDim).monospacedDigit()
            Button { store.addStackPane() } label: {
                Image(systemName: "plus").font(.system(size: 18, weight: .semibold)).foregroundStyle(Konjo.flame)
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
                draggablePane(pane, idx)
                    // `before` was hardcoded `true` — a no-op exactly when
                    // dragging pane 0 onto pane 1 (`moveStackBeforeOrAfter`'s
                    // `to = targetIndex - 1 = fromIndex`). Web decides
                    // before/after from the drop cursor's position relative
                    // to the target's midpoint; `location` (previously
                    // ignored) is the SwiftUI equivalent.
                    .background(GeometryReader { geo in
                        Color.clear.preference(key: PaneWidthKey.self, value: [idx: geo.size.width])
                    })
                    .dropDestination(for: StackDragPayload.self) { items, location in
                        guard let payload = items.first, payload.index != idx else { return false }
                        let width = paneWidths[idx] ?? 0
                        let before = width > 0 ? location.x < width / 2 : true
                        store.reorderStacksInPanes(payload.index, idx, before)
                        return true
                    }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .overlay(alignment: .top) { forgeBanner }
    }

    /// `.draggable()` attaches to the WHOLE pane only while
    /// `model.armedStackDragIndex` matches it — i.e. only for the duration
    /// its dock's drag handle (`StackControlDockView.dragHandle`) is
    /// pressed. Mirrors web's `armDrag`/`disarmDrag`. Attaching
    /// `.draggable()` unconditionally to the pane would make the whole
    /// pane a drag source all the time, breaking every button/text field
    /// inside it the same way it did on the drag handle itself.
    @ViewBuilder
    private func draggablePane(_ pane: StackPaneState, _ idx: Int) -> some View {
        let content = StackPaneView(
            store: store, engine: engine, pane: pane, index: idx, repoOptions: repoChoices,
            onClose: store.panes.count > 1 ? { closePane(pane.key) } : nil)
        if model.armedStackDragIndex == idx {
            content.draggable(StackDragPayload(index: idx))
        } else {
            content
        }
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
