import SwiftUI
import LopiStacksKit

/// StackPaneView — one pane's chrome: header (logo + title + status dot + close),
/// a pinned **draft** card (Creation-Flow-1 — the composer replacement; the thing
/// you compose is the card you'll get), the committed card stack flowing down to
/// the currently-executing loop at the bottom, and the purple stack control dock.
/// An empty pane reads as a bare box (composer + idle orb only); the dock and
/// inter-card connectors appear as soon as the pane holds its first card.
struct StackPaneView: View {
    var store: StackStore
    var engine: StackRunEngine
    var pane: StackPaneState
    var index: Int
    var repoOptions: [StackOption]
    var onClose: (() -> Void)?

    private var paneDefaults: StackDefaults { pane.config.defaults }
    private var scheduleGoverned: Bool { perLoopScheduleGoverned(pane.config) }
    private var bare: Bool { paneIsBare(pane) }

    var body: some View {
        VStack(spacing: 0) {
            header
            cardStack
            footer
        }
        .background(Konjo.panel)
        .clipShape(RoundedRectangle(cornerRadius: 14))
        .overlay(RoundedRectangle(cornerRadius: 14).stroke(Konjo.line2, lineWidth: 1))
    }

    // MARK: Header

    private var header: some View {
        HStack(spacing: 11) {
            LopiMark(size: 15, color: Konjo.flame)
            Text(pane.title.uppercased()).font(Konjo.mono(12)).tracking(1.6).foregroundStyle(Konjo.fg)
            Spacer(minLength: 0)
            Circle().fill(Konjo.fgMute).frame(width: 7, height: 7)
            Button { onClose?() } label: {
                Image(systemName: "xmark").font(.system(size: 12)).foregroundStyle(onClose == nil ? Konjo.fgMute : Konjo.fgDim)
            }
            .buttonStyle(.plain).disabled(onClose == nil).help("close pane")
        }
        .padding(.horizontal, 18).padding(.vertical, 14)
        .overlay(Rectangle().fill(Konjo.line).frame(height: 1), alignment: .bottom)
    }

    // MARK: Card stack — pinned draft, then committed loops (newest at top →
    //        oldest/next-to-run at the bottom)

    private var cardStack: some View {
        ScrollView {
            VStack(spacing: 2) {
                // The draft *is* the composer. Pinned at the top; it lives on
                // pane.draft (never in pane.cards), so it's excluded from
                // run/reorder/loop-count. The committed cards flow down below it.
                StackCardView(store: store, paneKey: pane.key, card: pane.draft, index: -1,
                              paneDefaults: paneDefaults, repoOptions: repoOptions,
                              scheduleGoverned: scheduleGoverned)
                if !pane.cards.isEmpty {
                    draftConnector
                    ForEach(Array(pane.cards.enumerated()), id: \.element.id) { i, card in
                        StackCardView(store: store, paneKey: pane.key, card: card, index: i,
                                      paneDefaults: paneDefaults, repoOptions: repoOptions, scheduleGoverned: scheduleGoverned)
                        if i < pane.cards.count - 1 {
                            StackConnectorView(store: store, paneKey: pane.key, card: card, index: i, scheduleGoverned: scheduleGoverned)
                        }
                    }
                }
            }
            .padding(.horizontal, 18).padding(.top, 24).padding(.bottom, 8)
        }
        .frame(maxHeight: .infinity)
    }

    /// The short visual connector between the pinned draft and the committed
    /// stack (purely visual — unlike StackConnectorView, no "add between" here).
    private var draftConnector: some View {
        DashedVLine()
            .stroke(Konjo.fgMute, style: StrokeStyle(lineWidth: 2, dash: [4, 3]))
            .frame(width: 2, height: 26)
            .padding(.vertical, 2)
    }

    // MARK: Footer — the purple dock. A bare (empty) pane has nothing to run yet.

    @ViewBuilder private var footer: some View {
        if !bare {
            StackControlDockView(store: store, engine: engine, pane: pane, index: index, repoOptions: repoOptions)
        }
    }
}

/// A one-segment vertical line, dashed via the caller's `StrokeStyle` — the
/// draft→stack connector in `StackPaneView`.
private struct DashedVLine: Shape {
    func path(in rect: CGRect) -> Path {
        var p = Path()
        p.move(to: CGPoint(x: rect.midX, y: rect.minY))
        p.addLine(to: CGPoint(x: rect.midX, y: rect.maxY))
        return p
    }
}
