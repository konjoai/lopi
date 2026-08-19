import SwiftUI

/// The app's root navigation — a custom bottom bar (`RootTabBar`) replacing
/// both SwiftUI's stock `TabView` chrome and the older header-icon-opens-a-
/// sheet pattern. Six tabs (Stacks/Schedule/Loop/Overview/Budget/Config) stay
/// mounted at all times in a `ZStack` with opacity toggling — not a
/// `switch` that recreates the losing view — so each tab keeps its own
/// scroll position, `NavigationStack` path, and local `@State` across
/// switches, matching what a real `TabView` gives you for free.
///
/// The bar is 3 tabs left (Stacks/Schedule/Loop) + 3 right
/// (Overview/Budget/Config) around the center action button — 7 slots on one
/// row is a lot for a phone width, so `RootTabBar`'s icon/label sizing was
/// tightened accordingly; worth a real-device glance for crowding.
///
/// "New stack" used to be a FAB owned by `StackOverviewScreen`; it's now the
/// bar's raised center button instead, so it works from any tab — the
/// create/cleanup logic moved up here accordingly.
struct RootTabView: View {
    /// Wraps the new pane's key so `.fullScreenCover(item:)` can key off it
    /// directly. A plain `String?` + a separate `showNewStack: Bool` (the
    /// original shape) is the classic SwiftUI race: two `@State` vars that
    /// must update in lockstep, where the cover can present before the key
    /// has propagated to the content closure — confirmed via a throwaway
    /// UI test to be a *real* race (reproducible blank screen), not
    /// hypothetical. `item:`-based presentation ties "whether to present"
    /// and "what to present" to the same value, so they can't desync.
    private struct NewStackTarget: Identifiable { let id: String }

    @Environment(AppModel.self) private var model
    @State private var selection: RootTab = .stacks
    @State private var newStackTarget: NewStackTarget?
    /// Survives past `newStackTarget` resetting to `nil` on dismiss, so
    /// `cleanupNewStackIfEmpty` still knows which pane to check.
    @State private var pendingCleanupKey: String?

    var body: some View {
        ZStack {
            tab(.stacks) { StackOverviewScreen() }
            tab(.schedule) { SchedulingScreen() }
            tab(.loop) { LoopScreen() }
            tab(.overview) { OverviewScreen() }
            tab(.budget) { BudgetScreen() }
            tab(.config) { ServerConfigScreen() }
        }
        .safeAreaInset(edge: .bottom, spacing: 0) {
            RootTabBar(selection: $selection, onNewStack: startNewStack)
        }
        .fullScreenCover(item: $newStackTarget, onDismiss: cleanupNewStackIfEmpty) { target in
            StackDetailScreen(paneKey: target.id)
        }
    }

    @ViewBuilder
    private func tab<Content: View>(_ tab: RootTab, @ViewBuilder content: () -> Content) -> some View {
        content()
            .opacity(selection == tab ? 1 : 0)
            .allowsHitTesting(selection == tab)
    }

    /// Creates the blank pane immediately (mirrors macOS `ForgeView`'s
    /// always-materialized empty pane) and opens it full-screen. Available
    /// from every tab now, not just Stacks.
    private func startNewStack() {
        model.stackStore.addStackPane()
        guard let key = model.stackStore.panes.last?.key else { return }
        pendingCleanupKey = key
        newStackTarget = NewStackTarget(id: key)
    }

    /// Tears the pane back down if the user backed out without committing a
    /// card, so cancelling never litters the Overview with empty stacks.
    private func cleanupNewStackIfEmpty() {
        guard let key = pendingCleanupKey else { return }
        pendingCleanupKey = nil
        if model.stackStore.pane(for: key)?.cards.isEmpty ?? true {
            model.stackEngine.clearRun(key)
            model.stackStore.deleteStackFromPanes(key)
        }
    }
}
