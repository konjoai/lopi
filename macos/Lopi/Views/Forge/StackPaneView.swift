import SwiftUI
import LopiStacksKit

/// StackPaneView — one pane's chrome: header (logo + title + status dot + close),
/// a pinned **draft** card (Creation-Flow-1 — the composer replacement; the thing
/// you compose is the card you'll get), the committed card stack flowing down to
/// the currently-executing loop at the bottom, and either the purple stack
/// control dock (2+ cards) or the bare-pane run button (≤1 card). Unify-2 §3: a
/// 0- or 1-card pane still reads like the old Forge pane; the dock and inter-card
/// connectors appear only once a second loop makes it a real stack.
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
    private var barePhase: RunPhase? { engine.run(for: pane.key)?.phase }

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

    // MARK: Footer — dock (stack) or bare-run (≤1 card)

    @ViewBuilder private var footer: some View {
        if !bare {
            StackControlDockView(store: store, engine: engine, pane: pane, index: index, repoOptions: repoOptions)
        } else if !pane.cards.isEmpty {
            HStack {
                Spacer()
                Button { engine.runBarePane(pane.key, PaneDefaults(paneDefaults)) } label: {
                    HStack(spacing: 8) {
                        Image(systemName: "play.fill").font(.system(size: 13, weight: .bold))
                        Text(barePhase == .running ? "running…" : "run").font(Konjo.sans(13, weight: .bold))
                    }
                    .padding(.horizontal, 26).padding(.vertical, 12)
                    .background(LinearGradient(colors: [Color(hex: 0xFFB648), Konjo.flame], startPoint: .top, endPoint: .bottom))
                    .foregroundStyle(Color(hex: 0x231000))
                    .clipShape(RoundedRectangle(cornerRadius: 9))
                    .shadow(color: Konjo.flame.opacity(0.28), radius: 9, y: 5)
                }
                .buttonStyle(.plain).disabled(barePhase == .running)
                Spacer()
            }
            .padding(.horizontal, 16).padding(.top, 13).padding(.bottom, 16)
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
