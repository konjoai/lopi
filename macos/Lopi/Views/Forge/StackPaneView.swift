import SwiftUI

/// StackPaneView — one pane's chrome: header (logo + title + status dot + close),
/// top composer (new prompts prepend), the card stack flowing down to the
/// currently-executing loop at the bottom, and either the purple stack control
/// dock (2+ cards) or the bare-pane run button (≤1 card). Unify-2 §3: a 0- or
/// 1-card pane is a *bare* box that reads like the old Forge pane; the dock and
/// inter-card connectors appear only once a second loop makes it a real stack.
struct StackPaneView: View {
    var store: StackStore
    var engine: StackRunEngine
    var pane: StackPaneState
    var index: Int
    var repoOptions: [StackOption]
    var onClose: (() -> Void)?

    @State private var composerValue = ""

    private var paneDefaults: StackDefaults { pane.config.defaults }
    private var scheduleGoverned: Bool { perLoopScheduleGoverned(pane.config) }
    private var bare: Bool { paneIsBare(pane) }
    private var barePhase: RunPhase? { engine.run(for: pane.key)?.phase }

    var body: some View {
        VStack(spacing: 0) {
            header
            composer
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
            Image(systemName: "circle.grid.2x2.fill").font(.system(size: 15)).foregroundStyle(Konjo.flame)
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

    // MARK: Composer (prepends)

    private var composer: some View {
        HStack(spacing: 11) {
            Text(">").font(Konjo.mono(15)).foregroundStyle(Konjo.flame)
            TextField("add a prompt or goal…", text: $composerValue)
                .textFieldStyle(.plain).font(Konjo.mono(14)).foregroundStyle(Konjo.fg)
                .onSubmit(submit)
            Button(action: submit) {
                Image(systemName: "plus").font(.system(size: 14))
                    .foregroundStyle(composerValue.trimmed.isEmpty ? Konjo.fgMute : Konjo.flame)
                    .frame(width: 34, height: 34)
                    .overlay(RoundedRectangle(cornerRadius: 7).stroke(Konjo.line2, lineWidth: 1))
            }
            .buttonStyle(.plain).disabled(composerValue.trimmed.isEmpty).help("add to stack")
        }
        .padding(.horizontal, 18).padding(.vertical, 13)
        .overlay(Rectangle().fill(Konjo.line).frame(height: 1), alignment: .bottom)
    }

    private func submit() {
        let text = composerValue.trimmed
        guard !text.isEmpty else { return }
        store.addToPane(pane.key, buildCard(text))
        composerValue = ""
    }

    // MARK: Card stack (newest at top → oldest/next-to-run at the bottom)

    private var cardStack: some View {
        ScrollView {
            VStack(spacing: 2) {
                if pane.cards.isEmpty {
                    emptyState
                } else {
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

    private var emptyState: some View {
        VStack(spacing: 6) {
            Text("no loops yet").font(Konjo.sans(13, weight: .semibold)).foregroundStyle(Konjo.fgDim)
            Text("add one above").font(Konjo.mono(10)).foregroundStyle(Konjo.fgMute)
        }
        .frame(maxWidth: .infinity).padding(.vertical, 40)
    }

    // MARK: Footer — dock (stack) or bare-run (≤1 card)

    @ViewBuilder private var footer: some View {
        if !bare {
            StackControlDockView(store: store, engine: engine, pane: pane, index: index, repoOptions: repoOptions)
        } else if !pane.cards.isEmpty {
            Button { engine.runBarePane(pane.key, PaneDefaults(paneDefaults)) } label: {
                HStack(spacing: 9) {
                    Image(systemName: "play.fill").font(.system(size: 13, weight: .bold))
                    Text(barePhase == .running ? "running…" : "run").font(Konjo.sans(13, weight: .bold))
                }
                .frame(maxWidth: .infinity).padding(.vertical, 12)
                .background(LinearGradient(colors: [Color(hex: 0xFFB648), Konjo.flame], startPoint: .top, endPoint: .bottom))
                .foregroundStyle(Color(hex: 0x231000))
                .clipShape(RoundedRectangle(cornerRadius: 8))
            }
            .buttonStyle(.plain).disabled(barePhase == .running)
            .padding(.horizontal, 16).padding(.top, 13).padding(.bottom, 16)
        }
    }
}

private extension String {
    var trimmed: String { trimmingCharacters(in: .whitespacesAndNewlines) }
}
