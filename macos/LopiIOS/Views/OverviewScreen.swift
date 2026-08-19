import SwiftUI
import LopiStacksKit

/// Overview — the read-only Loop Stacks board (Phase 5): every stack pane
/// grouped into four lifecycle columns (queued/running/testing/done),
/// kanban-style. The iOS counterpart of the (also in-progress) macOS
/// `OverviewView.swift` — same shared data layer (`Store/StackOverview.swift`'s
/// `buildStackOverviewCards`/`groupByLifecycle`/`StackLifecycle`/
/// `StackOverviewCard`, the Swift port of web's `stores/stackOverview.ts`),
/// zero new networking. Distinct from the "Stacks" tab: this is a passive
/// rollup (no swipe-to-pause/delete), and macOS's 4-wide horizontal kanban
/// columns become 4 vertical sections in one scrolling column here — portrait
/// width can't fit 4 columns side by side. Tapping a card still pushes into
/// `StackDetailScreen`, since drilling in is useful even from a read-only view.
///
/// Deliberately its own screen rather than a mode-toggle on `StackOverviewScreen`
/// (the open question the parity plan left unresolved) — the two screens
/// differ enough in both data shape and interaction model (kanban rollup vs.
/// swipe-managed list) that folding one into the other would need its own
/// mode-switch UI anyway, and macOS already treats them as separate views.
struct OverviewScreen: View {
    @Environment(AppModel.self) private var model
    @State private var path = NavigationPath()

    private var cards: [StackOverviewCard] { buildStackOverviewCards(model.stackStore.panes, model.liveAgents) }
    private var groups: [StackLifecycle: [StackOverviewCard]] { groupByLifecycle(cards) }
    private var liveCount: Int { (groups[.running]?.count ?? 0) + (groups[.testing]?.count ?? 0) }
    private var spent: Double { totalCost(model.liveAgents) }
    private var offline: Bool { model.connection == .offline || model.connection == .connecting }

    var body: some View {
        NavigationStack(path: $path) {
            Group {
                if cards.isEmpty {
                    emptyBanner
                } else {
                    board
                }
            }
            .background(Konjo.deep)
            .navigationTitle("")
            .toolbar(.hidden, for: .navigationBar)
            .safeAreaInset(edge: .top) { header }
            .navigationDestination(for: String.self) { key in
                if let pane = model.stackStore.pane(for: key) {
                    StackDetailScreen(paneKey: pane.key)
                }
            }
        }
    }

    // MARK: Header

    private var header: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .firstTextBaseline) {
                Text("Overview")
                    .font(Konjo.sans(22, weight: .heavy))
                    .foregroundStyle(Konjo.fg)
                Spacer()
                HStack(spacing: 5) {
                    Circle().fill(offline ? Konjo.fgMute : Konjo.jade).frame(width: 6, height: 6)
                    Text(offline ? "OFFLINE" : "LIVE")
                        .font(Konjo.mono(10)).tracking(1.2)
                        .foregroundStyle(offline ? Konjo.fgMute : Konjo.jade)
                }
            }
            HStack(spacing: 16) {
                statText("\(cards.count)", "stacks", Konjo.fg)
                statText("\(liveCount)", "live", Konjo.ice)
                statText(String(format: "$%.4f", spent), "spent", Konjo.fg)
            }
        }
        .padding(.horizontal, 18)
        .padding(.top, 8)
        .padding(.bottom, 10)
        .background(Konjo.deep)
    }

    private func statText(_ value: String, _ label: String, _ c: Color) -> some View {
        HStack(spacing: 4) {
            Text(value).font(Konjo.mono(11, weight: .bold)).foregroundStyle(c)
            Text(label).font(Konjo.mono(11)).foregroundStyle(Konjo.fgMute)
        }
    }

    private var emptyBanner: some View {
        Text("no stacks yet — add a prompt on the Stacks tab to put one on the board")
            .font(Konjo.mono(12))
            .foregroundStyle(Konjo.fgMute)
            .multilineTextAlignment(.center)
            .frame(maxWidth: .infinity)
            .padding(.vertical, 40)
            .padding(.horizontal, 24)
    }

    // MARK: Board — 4 vertical sections (macOS's 4 horizontal columns don't fit portrait)

    private var board: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                ForEach(StackLifecycle.allCases, id: \.self) { lifecycle in
                    column(lifecycle)
                }
            }
            .padding(16)
        }
    }

    private func column(_ lifecycle: StackLifecycle) -> some View {
        let items = groups[lifecycle] ?? []
        return VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 8) {
                Circle().fill(lifecycle.color).frame(width: 8, height: 8)
                Text(lifecycle.label.uppercased())
                    .font(Konjo.sans(13, weight: .semibold)).tracking(1)
                    .foregroundStyle(Konjo.fg)
                Spacer()
                Text("\(items.count)").font(Konjo.mono(11)).foregroundStyle(lifecycle.color)
            }
            .padding(.bottom, 6)
            .overlay(alignment: .bottom) { Rectangle().fill(lifecycle.color).frame(height: 2) }

            if items.isEmpty {
                Text("none")
                    .font(Konjo.mono(10.5)).foregroundStyle(Konjo.fgMute.opacity(0.6))
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 10)
                    .overlay(
                        RoundedRectangle(cornerRadius: 8)
                            .strokeBorder(style: StrokeStyle(lineWidth: 1, dash: [3, 3]))
                            .foregroundStyle(Konjo.line.opacity(0.5))
                    )
            } else {
                VStack(spacing: 8) {
                    ForEach(items) { card in
                        OverviewCardRow(card: card) {
                            Haptics.tap()
                            path.append(card.key)
                        }
                    }
                }
            }
        }
    }
}

/// One board card — left-accent bar, name + live pulse dot, single-line
/// goal, mini loop-progress dots, and a single right-aligned meta value
/// (repo while queued, elapsed+cost while live, cost/failed once done).
/// iOS-local rather than reusing macOS's `StackOverviewCardView.swift` —
/// that file is part of the in-progress macOS Overview work, not yet
/// stable/shared; this is a fresh port from the same `StackOverviewCard`
/// data model.
private struct OverviewCardRow: View {
    let card: StackOverviewCard
    var onTap: () -> Void

    private var isLive: Bool { card.lifecycle == .running || card.lifecycle == .testing }
    private var metaText: String { card.lifecycle == .queued ? card.repo : card.metaRight }
    private var metaColor: Color { card.lifecycle == .queued ? Konjo.fgMute.opacity(0.5) : card.metaRightColor }

    var body: some View {
        Button(action: onTap) {
            HStack(alignment: .top, spacing: 10) {
                VStack(alignment: .leading, spacing: 4) {
                    HStack(spacing: 6) {
                        Text(card.title)
                            .font(Konjo.sans(12.5, weight: .semibold))
                            .foregroundStyle(Konjo.fg)
                            .lineLimit(1)
                        if isLive {
                            PulsingDot(color: card.accentColor, pulsing: true, size: 5)
                        }
                    }
                    Text(card.goal)
                        .font(Konjo.sans(11))
                        .foregroundStyle(Konjo.fgDim)
                        .lineLimit(1)
                        .truncationMode(.tail)
                    HStack(spacing: 4) {
                        ForEach(card.loops) { loop in
                            PulsingDot(color: loop.color, pulsing: loop.pulsing, size: 6)
                        }
                    }
                    .padding(.top, 3)
                }
                Spacer(minLength: 0)
                Text(metaText)
                    .font(Konjo.mono(9))
                    .foregroundStyle(metaColor)
                    .lineLimit(1)
                    .fixedSize(horizontal: true, vertical: false)
            }
            .padding(.horizontal, 10).padding(.vertical, 12)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(card.accentColor.opacity(0.08))
            .overlay(alignment: .leading) {
                Rectangle().fill(card.accentColor).frame(width: 3)
            }
            .clipShape(UnevenRoundedRectangle(topLeadingRadius: 0, bottomLeadingRadius: 0, bottomTrailingRadius: 8, topTrailingRadius: 8))
        }
        .buttonStyle(.plain)
    }
}
