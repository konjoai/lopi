import SwiftUI

/// `OverviewView`'s board's per-stack card. Dense rows-in-columns layout:
/// left-accent bar, name + live dot, single-line goal, mini loop-progress
/// dots, and a single right-aligned meta value (repo while queued,
/// elapsed+cost while live, cost/failed once done). Mirrors web's
/// `StackOverviewCard.svelte`.
struct StackOverviewCardView: View {
    let card: StackOverviewCard
    var onOpen: () -> Void

    private var isLive: Bool { card.lifecycle == .running || card.lifecycle == .testing }

    var body: some View {
        Button(action: onOpen) {
            HStack(alignment: .top, spacing: 10) {
                VStack(alignment: .leading, spacing: 4) {
                    HStack(spacing: 6) {
                        Text(card.title)
                            .font(Konjo.sans(12.5, weight: .semibold))
                            .foregroundStyle(Konjo.fg)
                            .lineLimit(1)
                        if isLive {
                            PulsingDot(color: card.accentColor, size: 5)
                        }
                    }
                    Text(card.goal)
                        .font(Konjo.mono(11))
                        .foregroundStyle(Konjo.fg.opacity(0.65))
                        .lineLimit(1).truncationMode(.tail)
                    loopDots
                }
                Spacer(minLength: 0)
                Text(card.lifecycle == .queued ? card.repo : card.metaRight)
                    .font(Konjo.mono(9))
                    .foregroundStyle(card.lifecycle == .queued ? Konjo.fg.opacity(0.35) : card.metaRightColor)
                    .lineLimit(1)
            }
            .padding(.horizontal, 10).padding(.vertical, 12)
            .background(
                RoundedRectangle(cornerRadius: 8)
                    .fill(card.accentColor.opacity(0.08))
            )
            .overlay(alignment: .leading) {
                RoundedRectangle(cornerRadius: 1.5).fill(card.accentColor).frame(width: 3)
            }
            .clipShape(RoundedRectangle(cornerRadius: 8))
        }
        .buttonStyle(.plain)
    }

    private var loopDots: some View {
        HStack(spacing: 4) {
            ForEach(card.loops) { loop in
                if loop.pulsing {
                    PulsingDot(color: loop.color, size: 6)
                } else {
                    Circle().fill(loop.color).frame(width: 6, height: 6)
                }
            }
        }
        .padding(.top, 3)
    }
}

/// A small circle that fades in and out forever — the board's "this is
/// actively live" tell (card name dot, running loop segment). Mirrors web's
/// `cardpulse` keyframes.
private struct PulsingDot: View {
    let color: Color
    let size: CGFloat
    @State private var dim = false

    var body: some View {
        Circle().fill(color).frame(width: size, height: size)
            .opacity(dim ? 0.4 : 1)
            .onAppear {
                withAnimation(.easeInOut(duration: 1.8).repeatForever(autoreverses: true)) { dim = true }
            }
    }
}
