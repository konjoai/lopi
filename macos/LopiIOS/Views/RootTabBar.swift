import SwiftUI

/// The tab identity `RootTabView` switches on. `newStack` isn't a real tab
/// (no content view of its own) — it's the center bar button's identity,
/// kept in the same enum only so `RootTabBar` can treat all five bar slots
/// uniformly for layout.
enum RootTab: Hashable {
    case stacks, schedule, loop, overview, budget, config
}

/// A custom bottom bar (not SwiftUI's `TabView`/`.tabItem`) matching the
/// reference design: four flat tab buttons flanking a raised, circular,
/// flame-colored center action button. `TabView` has no supported way to
/// render a non-tab action button in the middle of the bar, so this is a
/// hand-built `HStack` instead — same visual weight, but the center slot
/// calls `onNewStack()` directly rather than changing `selection`.
struct RootTabBar: View {
    @Binding var selection: RootTab
    let onNewStack: () -> Void

    var body: some View {
        HStack(spacing: 0) {
            tabButton(.stacks, "arrow.triangle.2.circlepath", "Stacks")
            tabButton(.schedule, "clock", "Schedule")
            tabButton(.loop, "gearshape.2", "Loop")
            newStackButton
            tabButton(.overview, "square.grid.2x2", "Overview")
            tabButton(.budget, "dollarsign.circle", "Budget")
            tabButton(.config, "gearshape", "Config")
        }
        .padding(.horizontal, 2)
        .padding(.top, 10)
        .padding(.bottom, 6)
        .background(Konjo.deep, ignoresSafeAreaEdges: .bottom)
        .overlay(alignment: .top) { Rectangle().fill(Konjo.line).frame(height: 1) }
    }

    private func tabButton(_ tab: RootTab, _ systemImage: String, _ label: String) -> some View {
        let active = selection == tab
        return Button {
            guard selection != tab else { return }
            Haptics.selection()
            selection = tab
        } label: {
            VStack(spacing: 2) {
                Image(systemName: systemImage).font(.system(size: 17))
                Text(label).font(Konjo.mono(8, weight: active ? .semibold : .regular))
                    .lineLimit(1)
                    .minimumScaleFactor(0.8)
            }
            .foregroundStyle(active ? Konjo.fg : Konjo.fgMute)
            .frame(maxWidth: .infinity)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    /// Raised above the bar line (matches the reference screenshot) and kept
    /// flame-colored regardless of `selection` — it's an action, not a
    /// selected/unselected state like the other four.
    private var newStackButton: some View {
        Button {
            Haptics.impact()
            onNewStack()
        } label: {
            Image(systemName: "plus")
                .font(.system(size: 22, weight: .bold))
                .foregroundStyle(Color(hex: 0x1A0F00))
                .frame(width: 54, height: 54)
                .background(
                    LinearGradient(colors: [Konjo.flame, Color(hex: 0xE6820A)], startPoint: .top, endPoint: .bottom),
                    in: Circle()
                )
                .overlay(Circle().stroke(Konjo.deep, lineWidth: 4))
                .shadow(color: Konjo.flame.opacity(0.45), radius: 10, y: 4)
        }
        .buttonStyle(.plain)
        .frame(maxWidth: .infinity)
        .offset(y: -16)
        .accessibilityIdentifier("tabbar.newStack")
    }
}
