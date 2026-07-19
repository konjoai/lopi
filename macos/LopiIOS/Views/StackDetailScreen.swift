import SwiftUI
import LopiStacksKit

/// One stack's working view — write a new prompt, watch the chain of loop
/// cards, control the whole run. The iOS mobile counterpart to
/// `StackControlDockView.swift`/`StackCardView.swift` (macOS) and
/// `StackControlDock.svelte`/`StackCard.svelte` (web); matches the design
/// handoff's "popover cardbar" variant (`LoopStackPopover`).
struct StackDetailScreen: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let paneKey: String

    private var pane: StackPaneState? { model.stackStore.pane(for: paneKey) }

    var body: some View {
        VStack(spacing: 0) {
            header
            if let pane {
                ScrollView {
                    VStack(spacing: 0) {
                        ComposerCardView(paneKey: pane.key)
                        StackConnector(dashed: true)
                        ForEach(Array(pane.cards.enumerated()), id: \.element.id) { index, card in
                            LoopCardView(paneKey: pane.key, card: card)
                            if index < pane.cards.count - 1 {
                                StackConnector(dashed: false)
                            }
                        }
                    }
                    .padding(.horizontal, 16)
                    .padding(.top, 20)
                    .padding(.bottom, 8)
                }
                stackDock(pane)
            }
        }
        .background(Konjo.panel)
        .toolbar(.hidden, for: .navigationBar)
    }

    private var header: some View {
        HStack(spacing: 11) {
            Image(systemName: "arrow.triangle.2.circlepath")
                .font(.system(size: 15))
                .foregroundStyle(Konjo.flame)
            Text(pane?.title ?? "stack")
                .font(Konjo.mono(12, weight: .bold))
                .tracking(2)
                .textCase(.uppercase)
                .foregroundStyle(Konjo.fg)
            Spacer()
            Circle().fill(Konjo.fgMute).frame(width: 7, height: 7)
            Button { dismiss() } label: {
                Image(systemName: "xmark").font(.system(size: 13)).foregroundStyle(Konjo.fgMute)
            }
            .buttonStyle(.plain)
        }
        .padding(14)
        .overlay(alignment: .bottom) { Rectangle().fill(Konjo.line).frame(height: 1) }
    }

    private func stackDock(_ pane: StackPaneState) -> some View {
        let runningTotal = pane.cards.compactMap { $0.taskId }
            .compactMap { model.liveAgents[$0] }
            .reduce(0.0) { $0 + $1.costUsd }
        return VStack(alignment: .leading, spacing: 9) {
            HStack(spacing: 8) {
                Text("STACK")
                    .font(Konjo.mono(9, weight: .bold))
                    .tracking(0.5)
                    .foregroundStyle(.white)
                    .padding(.horizontal, 7).padding(.vertical, 2)
                    .background(Konjo.violet, in: RoundedRectangle(cornerRadius: 5))
                Text("running total: ")
                    .font(Konjo.mono(10.5)).foregroundStyle(Konjo.fgDim)
                + Text(String(format: "$%.2f", runningTotal))
                    .font(Konjo.mono(10.5, weight: .bold)).foregroundStyle(Konjo.fg)
                Spacer()
                Image(systemName: "chevron.down").font(.system(size: 11)).foregroundStyle(Konjo.fgMute)
            }

            Text("stack command…")
                .font(Konjo.sans(12.5))
                .foregroundStyle(Konjo.fgMute)
                .padding(.horizontal, 11).padding(.vertical, 9)
                .frame(maxWidth: .infinity, alignment: .leading)
                .overlay(RoundedRectangle(cornerRadius: 8).stroke(Konjo.violet.opacity(0.3), lineWidth: 1))

            HStack(spacing: 6) {
                GrammarChip(label: ":alias", color: Konjo.stackTeal)
                GrammarChip(label: "@repo", color: Konjo.ice)
                GrammarChip(label: "/model", color: Konjo.stackViolet)
                GrammarChip(label: "/effort", color: Konjo.flame)
                GrammarChip(label: "×N", color: Konjo.sun)
            }

            StackDockCardBar(paneKey: pane.key)

            Button {
                model.stackEngine.runStack(paneKey, .run, PaneDefaults(pane.config.defaults))
            } label: {
                HStack(spacing: 6) {
                    Image(systemName: "play.fill").font(.system(size: 12))
                    Text("run stack").font(Konjo.sans(14, weight: .bold))
                }
                .foregroundStyle(Color(hex: 0x1A0F00))
                .frame(maxWidth: .infinity)
                .padding(.vertical, 12)
                .background(
                    LinearGradient(colors: [Konjo.flame, Color(hex: 0xE6820A)], startPoint: .top, endPoint: .bottom),
                    in: RoundedRectangle(cornerRadius: 10)
                )
            }
            .buttonStyle(.plain)
        }
        .padding(.horizontal, 16)
        .padding(.top, 12)
        .padding(.bottom, 16)
        .background(
            LinearGradient(
                colors: [Konjo.violet.opacity(0.12), Konjo.violet.opacity(0.04)],
                startPoint: .top, endPoint: .bottom
            )
        )
        .overlay(alignment: .top) { Rectangle().fill(Konjo.violet.opacity(0.3)).frame(height: 1) }
    }
}

/// The stack-level cardbar in the dock — same chrome as a card's, minus the
/// duplicate/drag affordances (a stack itself isn't reorderable within
/// itself).
private struct StackDockCardBar: View {
    @Environment(AppModel.self) private var model
    let paneKey: String
    @State private var popoverOpen = false

    var body: some View {
        HStack(spacing: 6) {
            IterationPill(label: "off")
            Button { popoverOpen = true } label: {
                Text("•••")
                    .font(Konjo.mono(10.5))
                    .foregroundStyle(Konjo.fgDim)
                    .padding(.horizontal, 10)
                    .frame(height: 26)
                    .overlay(RoundedRectangle(cornerRadius: 6).stroke(Konjo.line2, lineWidth: 1))
            }
            .buttonStyle(.plain)
            .popover(isPresented: $popoverOpen) {
                FacetPopoverContent().presentationCompactAdaptation(.popover)
            }
            Spacer()
        }
    }
}

/// The composer — the draft card a new prompt is written into before
/// `commitDraft` turns it into a real (`.idle`) `StackCard`.
private struct ComposerCardView: View {
    @Environment(AppModel.self) private var model
    let paneKey: String
    @State private var popoverOpen = false

    private var draftGoal: Binding<String> {
        Binding(
            get: { model.stackStore.pane(for: paneKey)?.draft.goal ?? "" },
            set: { newValue in model.stackStore.updateDraftInPane(paneKey) { $0.goal = newValue } }
        )
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            RunTag(label: "new prompt", color: Konjo.fgMute, background: Konjo.panel)

            // Placeholder — full template-menu wiring (StackTemplateStore) is
            // real follow-up work, not part of this screens pass.
            Button {} label: {
                HStack(spacing: 5) {
                    Image(systemName: "list.bullet.rectangle").font(.system(size: 10))
                    Text("templates").font(Konjo.mono(10.5))
                    Image(systemName: "chevron.down").font(.system(size: 8))
                }
                .foregroundStyle(Konjo.fgDim)
                .padding(.horizontal, 9).padding(.vertical, 4)
                .overlay(Capsule().stroke(Konjo.line2, lineWidth: 1))
            }
            .buttonStyle(.plain)

            TextField("describe the prompt or goal...", text: draftGoal, axis: .vertical)
                .font(Konjo.sans(13))
                .foregroundStyle(Konjo.fg)
                .padding(9)
                .background(Color.white.opacity(0.02))
                .clipShape(RoundedRectangle(cornerRadius: 7))
                .overlay(RoundedRectangle(cornerRadius: 7).stroke(Konjo.line2, lineWidth: 1))

            HStack(spacing: 6) {
                GrammarChip(label: ":alias", color: Konjo.stackTeal)
                GrammarChip(label: "@repo", color: Konjo.ice)
                GrammarChip(label: "/model", color: Konjo.stackViolet)
                GrammarChip(label: "/effort", color: Konjo.flame)
                GrammarChip(label: "×N", color: Konjo.sun)
            }

            HStack(spacing: 6) {
                IterationPill(label: "off")
                Button { popoverOpen = true } label: {
                    Text("•••")
                        .font(Konjo.mono(10.5))
                        .foregroundStyle(Konjo.fgDim)
                        .padding(.horizontal, 10)
                        .frame(height: 26)
                        .overlay(RoundedRectangle(cornerRadius: 6).stroke(Konjo.line2, lineWidth: 1))
                }
                .buttonStyle(.plain)
                .popover(isPresented: $popoverOpen) {
                    FacetPopoverContent().presentationCompactAdaptation(.popover)
                }
                Spacer()
                Button {
                    model.stackStore.commitDraft(paneKey, repoOptions: repoOptions(model.repos))
                } label: {
                    Text("+ add")
                        .font(Konjo.mono(10)).foregroundStyle(Konjo.fgMute)
                        .padding(.horizontal, 11).padding(.vertical, 5)
                        .overlay(RoundedRectangle(cornerRadius: 6).stroke(Konjo.line2, lineWidth: 1))
                }
                .buttonStyle(.plain)
            }
        }
        .padding(14)
        .overlay(
            RoundedRectangle(cornerRadius: 9)
                .strokeBorder(style: StrokeStyle(lineWidth: 1.5, dash: [5, 4]))
                .foregroundStyle(Konjo.line2)
        )
    }
}

/// One committed loop card — a real `StackCard` in the pane. Long-press +
/// drag to reorder (native `.draggable`/`.dropDestination`, mirroring the
/// macOS `StackCardView` pattern).
private struct LoopCardView: View {
    @Environment(AppModel.self) private var model
    let paneKey: String
    let card: StackCard
    @State private var popoverOpen = false

    private var status: LoopCardDisplayStatus {
        StackDisplay.cardStatus(card, liveAgents: model.liveAgents)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            RunTag(label: status.label, color: status.color, background: Color(hex: 0x0E1214))

            Text("\u{201C}\(card.goal)\u{201D}")
                .font(Konjo.mono(13))
                .foregroundStyle(Konjo.fgDim)

            if case .blocked(let reason) = status {
                HStack(spacing: 6) {
                    Image(systemName: "xmark.circle.fill").font(.system(size: 10))
                    Text("eval failed: \(reason)").font(Konjo.mono(10))
                }
                .foregroundStyle(Color(hex: 0xFFAACB))
                .padding(.horizontal, 10).padding(.vertical, 8)
                .background(Konjo.rose.opacity(0.08), in: RoundedRectangle(cornerRadius: 7))
                .overlay(RoundedRectangle(cornerRadius: 7).stroke(Konjo.rose.opacity(0.25), lineWidth: 1))
            }

            HStack(spacing: 6) {
                IterationPill(label: card.maxIterations == 0 ? "off" : "×\(card.maxIterations)")
                Button { popoverOpen = true } label: {
                    HStack(spacing: 4) {
                        Text("•••").font(Konjo.mono(10.5))
                        if facetCount > 0 {
                            Text("\(facetCount)")
                                .font(Konjo.mono(8, weight: .semibold))
                                .padding(.horizontal, 4)
                                .background(Color.white.opacity(0.12), in: Capsule())
                        }
                    }
                    .foregroundStyle(Konjo.fgDim)
                    .padding(.horizontal, 10)
                    .frame(height: 26)
                    .overlay(RoundedRectangle(cornerRadius: 6).stroke(Konjo.line2, lineWidth: 1))
                }
                .buttonStyle(.plain)
                .popover(isPresented: $popoverOpen) {
                    FacetPopoverContent().presentationCompactAdaptation(.popover)
                }
                Spacer()
                CardIconButton(systemImage: "square.on.square") {
                    model.stackStore.duplicateInPane(paneKey, card.id)
                }
                CardIconButton(systemImage: "line.3.horizontal", active: true) {}
                CardIconButton(systemImage: "trash") {
                    model.stackStore.removeFromPane(paneKey, card.id)
                }
            }
        }
        .padding(14)
        .background(Color(hex: 0x0E1214))
        .clipShape(RoundedRectangle(cornerRadius: 9))
        .overlay(RoundedRectangle(cornerRadius: 9).stroke(borderColor, lineWidth: 1))
        .draggable(card.id)
        .dropDestination(for: String.self) { items, _ in
            guard let draggedId = items.first,
                  let pane = model.stackStore.pane(for: paneKey),
                  let from = pane.cards.firstIndex(where: { $0.id == draggedId }),
                  let to = pane.cards.firstIndex(where: { $0.id == card.id }),
                  from != to
            else { return false }
            model.stackStore.reorderInPane(paneKey, from, to)
            return true
        }
    }

    private var borderColor: Color {
        switch status {
        case .blocked: return Konjo.rose.opacity(0.45)
        case .done: return Konjo.jade.opacity(0.45)
        case .testing: return Konjo.violet.opacity(0.4)
        case .running: return Konjo.ice.opacity(0.4)
        case .queued: return Konjo.line
        }
    }

    private var facetCount: Int {
        var n = 0
        if card.scheduled { n += 1 }
        if card.guardrails.gate || card.guardrails.until { n += 1 }
        if card.evals.count > 1 { n += 1 }
        if card.maxx.enabled { n += 1 }
        return n
    }
}

/// The vertical connector between the composer/cards — dashed above the
/// first committed card, solid flame between subsequent ones.
private struct StackConnector: View {
    let dashed: Bool

    var body: some View {
        DashedVLine()
            .stroke(
                dashed ? Konjo.fgMute.opacity(0.28) : Konjo.flame,
                style: StrokeStyle(lineWidth: 2, dash: dashed ? [4, 4] : [])
            )
            .frame(height: dashed ? 26 : 20)
            .frame(maxWidth: .infinity)
    }
}

private struct DashedVLine: Shape {
    func path(in rect: CGRect) -> Path {
        var path = Path()
        path.move(to: CGPoint(x: rect.midX, y: rect.minY))
        path.addLine(to: CGPoint(x: rect.midX, y: rect.maxY))
        return path
    }
}

/// The facet tab-strip popover — schedule/guardrails/evals/goal/MAXX/config.
/// Content panes are placeholder copy in this pass (matches the design
/// handoff's own note: "wire to the real facet UI... in the native build");
/// wiring each to `SchedulePopoverView`/`GuardrailsPopoverView`/etc.'s real
/// controls is real follow-up work once this screen shape is confirmed.
private struct FacetPopoverContent: View {
    @State private var tab: CardFacet = .schedule

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 0) {
                ForEach(CardFacet.allCases) { facet in
                    Button { tab = facet } label: {
                        Image(systemName: facet.systemImage)
                            .font(.system(size: 13))
                            .foregroundStyle(tab == facet ? Konjo.fg : Konjo.fgMute)
                            .frame(maxWidth: .infinity)
                            .padding(.vertical, 8)
                            .background(tab == facet ? Color.white.opacity(0.06) : .clear)
                            .overlay(alignment: .bottom) {
                                Rectangle().fill(tab == facet ? Konjo.ice : .clear).frame(height: 2)
                            }
                    }
                    .buttonStyle(.plain)
                }
            }
            .overlay(alignment: .bottom) { Rectangle().fill(Konjo.line).frame(height: 1) }

            VStack(alignment: .leading, spacing: 6) {
                Text(tab.label.uppercased())
                    .font(Konjo.mono(10.5, weight: .bold))
                    .tracking(1)
                    .foregroundStyle(Konjo.fg)
                Text("tap to configure \(tab.label) for this loop")
                    .font(Konjo.sans(11.5))
                    .foregroundStyle(Konjo.fgMute)
            }
            .padding(14)
        }
        .frame(width: 240)
        .background(Color(hex: 0x101418))
    }
}

/// The six facets a card/stack can configure, matching `FacetAccent`
/// (`Stacks/StackTheme.swift`) for color consistency with macOS/web.
private enum CardFacet: String, CaseIterable, Identifiable {
    case schedule, guardrails, evals, goal, maxx, config

    var id: String { rawValue }
    var label: String { rawValue }

    var systemImage: String {
        switch self {
        case .schedule: return "clock"
        case .guardrails: return "checkmark.shield"
        case .evals: return "checklist"
        case .goal: return "target"
        case .maxx: return "bolt.fill"
        case .config: return "gearshape"
        }
    }
}
