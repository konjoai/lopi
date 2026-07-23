import SwiftUI
import LopiStacksKit

/// Overview — the Loop Stacks board: every stack across the app, grouped into
/// four lifecycle columns (queued/running/testing/done), kanban-style. Mirrors
/// web's `/overview` route (`StackOverview.swift`'s `buildStackOverviewCards`/
/// `groupByLifecycle`/`totalCost` are the pure-ish Swift port of
/// `stores/stackOverview.ts`).
///
/// Every card is a real client-side stack from `model.stackStore.panes`,
/// resolved against the live `liveAgents` map — no fabricated stacks. A stack
/// with no cards yet (still just an open composer) doesn't appear; add its
/// first prompt on the Forge grid to put it on the board.
struct OverviewView: View {
    @Environment(AppModel.self) private var model
    /// Opens the given stack (pane key) on the Forge grid — supplied by
    /// `RootView`, which owns the `selection` this screen doesn't.
    var onOpenStack: (String) -> Void

    private var cards: [StackOverviewCard] { buildStackOverviewCards(model.stackStore.panes, model.liveAgents) }
    private var groups: [StackLifecycle: [StackOverviewCard]] { groupByLifecycle(cards) }
    private var liveCount: Int { (groups[.running]?.count ?? 0) + (groups[.testing]?.count ?? 0) }
    private var spent: Double { totalCost(model.liveAgents) }
    private var offline: Bool { model.connection == .offline || model.connection == .connecting }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            header
            if cards.isEmpty {
                emptyState(offline
                    ? "start `lopi sail` to see live stacks"
                    : "no stacks yet — add a prompt on the Forge grid to put one on the board")
            } else {
                board
            }
        }
        .padding(24)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .background(Konjo.bg)
    }

    // MARK: Header

    private var header: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .firstTextBaseline) {
                Text("STACK LOOPS")
                    .font(Konjo.sans(24, weight: .bold))
                    .foregroundStyle(Konjo.fg)
                    .tracking(3)
                Spacer()
                ConnectionLED(state: model.connection)
            }
            HStack(spacing: 18) {
                statPill("\(cards.count)", "stacks", Konjo.fg)
                statPill("\(liveCount)", "live", Konjo.ice)
                statPill(String(format: "$%.4f", spent), "spent", Konjo.fg)
            }
        }
    }

    private func statPill(_ value: String, _ label: String, _ color: Color) -> some View {
        HStack(spacing: 4) {
            Text(value).font(Konjo.mono(11, weight: .bold)).foregroundStyle(color)
            Text(label).font(Konjo.mono(11)).foregroundStyle(Konjo.fgMute)
        }
    }

    private func emptyState(_ text: String) -> some View {
        Text(text)
            .font(Konjo.sans(13))
            .foregroundStyle(Konjo.fgMute)
            .padding(.vertical, 40)
            .frame(maxWidth: .infinity, alignment: .center)
    }

    // MARK: Board — four lifecycle columns

    private var board: some View {
        HStack(alignment: .top, spacing: 16) {
            ForEach(StackLifecycle.allCases) { lifecycle in
                column(lifecycle, groups[lifecycle] ?? [])
                    .frame(maxWidth: .infinity, alignment: .topLeading)
            }
        }
        .frame(maxHeight: .infinity, alignment: .top)
    }

    private func column(_ lifecycle: StackLifecycle, _ cards: [StackOverviewCard]) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            columnHeader(lifecycle, cards.count)
            ScrollView {
                VStack(spacing: 10) {
                    if cards.isEmpty {
                        Text("none")
                            .font(Konjo.mono(10.5))
                            .foregroundStyle(Konjo.fg.opacity(0.25))
                            .frame(maxWidth: .infinity)
                            .padding(.vertical, 10)
                            .overlay(RoundedRectangle(cornerRadius: 8).stroke(Konjo.line, style: StrokeStyle(lineWidth: 1, dash: [3, 3])))
                    } else {
                        ForEach(cards) { card in
                            StackOverviewCardView(card: card) { onOpenStack(card.key) }
                        }
                    }
                }
                .padding(.top, 12)
            }
        }
    }

    private func columnHeader(_ lifecycle: StackLifecycle, _ count: Int) -> some View {
        HStack(spacing: 8) {
            Circle().fill(lifecycle.color).frame(width: 8, height: 8)
            Text(lifecycle.label.uppercased())
                .font(Konjo.sans(12, weight: .semibold)).tracking(0.9)
                .foregroundStyle(Konjo.fg)
            Spacer(minLength: 0)
            Text("\(count)").font(Konjo.mono(11)).foregroundStyle(lifecycle.color)
        }
        .padding(.bottom, 10)
        .overlay(alignment: .bottom) { Rectangle().fill(lifecycle.color).frame(height: 2) }
    }
}
