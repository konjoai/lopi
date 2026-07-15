import SwiftUI
import LopiStacksKit

/// StackConnectorView — the vertical gap between two cards. A flame line, dashed
/// with a cyan cadence badge when the card above is scheduled (and not governed
/// by the stack), or a violet budget badge when its budget preset sets a real
/// cap. Hovering the gap reveals a dashed "add between" button that inserts a
/// fresh card right here via `insertCardIntoPane`.
struct StackConnectorView: View {
    var store: StackStore
    var paneKey: String
    /// The card above this gap — its schedule/budget drives the badge.
    var card: StackCard
    /// This card's index; the new card lands right after it.
    var index: Int
    var scheduleGoverned: Bool

    @State private var hovering = false

    private var sched: Bool { card.scheduled && !scheduleGoverned }
    private var budgetCap: Int? { budgetToTokens(card.guardrails.budget) }

    var body: some View {
        ZStack {
            line
            if sched {
                badge(systemImage: "clock", text: cronHuman(card.cron), color: Konjo.ice)
            } else if budgetCap != nil {
                badge(systemImage: "gauge", text: "budget \(card.guardrails.budget.rawValue)", color: Konjo.budgetViolet)
            }
            if hovering {
                Button { store.insertCardIntoPane(paneKey, index + 1, buildCard("new prompt")) } label: {
                    Image(systemName: "plus").font(.system(size: 14, weight: .semibold)).foregroundStyle(Konjo.ice)
                        .frame(maxWidth: .infinity).frame(height: 30)
                        .background(Konjo.ice.opacity(0.05))
                        .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(Konjo.ice.opacity(0.5), style: StrokeStyle(lineWidth: 1.5, dash: [4])))
                }
                .buttonStyle(.plain)
                .padding(.horizontal, 2)
            }
        }
        .frame(height: sched ? 72 : 52)
        .contentShape(Rectangle())
        .onHover { hovering = $0 }
    }

    /// The vertical spine — solid flame when unscheduled, dashed muted when the
    /// card above runs on its own cron.
    @ViewBuilder private var line: some View {
        if sched {
            Rectangle().fill(Konjo.fgMute.opacity(0.55))
                .frame(width: 2)
                .overlay(Rectangle().strokeBorder(style: StrokeStyle(lineWidth: 2, dash: [4])).foregroundStyle(Konjo.fgMute))
        } else {
            Rectangle().fill(FacetAccent.iteration.opacity(0.45)).frame(width: 2)
        }
    }

    private func badge(systemImage: String, text: String, color: Color) -> some View {
        HStack(spacing: 5) {
            Image(systemName: systemImage).font(.system(size: 9))
            Text(text).font(Konjo.mono(9.5)).lineLimit(1)
        }
        .foregroundStyle(color)
        .padding(.horizontal, 12).padding(.vertical, 4)
        .background(Capsule().fill(Konjo.deep))
        .overlay(Capsule().stroke(color.opacity(0.45), lineWidth: 1))
    }
}
