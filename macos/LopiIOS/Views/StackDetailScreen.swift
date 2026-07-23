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
                StackDockView(paneKey: pane.key)
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

}

// `StackDockView` (the "STACK" header, running total, collapse chevron,
// command bar, cardbar, and run button) lives in `StackCommandBar.swift`.

/// The composer — the draft card a new prompt is written into before
/// `commitDraft` turns it into a real (`.idle`) `StackCard`.
private struct ComposerCardView: View {
    @Environment(AppModel.self) private var model
    let paneKey: String
    @State private var templatesOpen = false
    @State private var popoverOpen = false
    @State private var savePromptAlert = false
    @State private var nameInput = ""

    private var draftGoal: Binding<String> {
        Binding(
            get: { model.stackStore.pane(for: paneKey)?.draft.goal ?? "" },
            set: { newValue in model.stackStore.updateDraftInPane(paneKey) { $0.goal = newValue } }
        )
    }

    private var hot: Bool {
        model.stackStore.pane(for: paneKey).map { draftIsHot($0.draft) } ?? false
    }

    private func writeDraft(_ mutate: (inout StackCard) -> Void) {
        model.stackStore.updateDraftInPane(paneKey, mutate)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            RunTag(label: "new prompt", color: Konjo.fgMute, background: Konjo.panel)

            Button { templatesOpen = true } label: {
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
            .popover(isPresented: $templatesOpen) {
                TemplatesMenuContent(
                    hot: hot,
                    prompts: model.stackTemplateStore.library.prompts,
                    onSelectPreset: { key in
                        writeDraft { $0 = applyPreset(key, to: $0) }
                        templatesOpen = false
                    },
                    onSelectPrompt: { tpl in
                        writeDraft { $0 = applyPromptTemplate(tpl, to: $0) }
                        templatesOpen = false
                    },
                    onSave: {
                        templatesOpen = false
                        nameInput = ""
                        savePromptAlert = true
                    }
                )
                .presentationCompactAdaptation(.popover)
            }
            .alert("Name this prompt template", isPresented: $savePromptAlert) {
                TextField("name", text: $nameInput)
                Button("Save") {
                    let name = nameInput.trimmingCharacters(in: .whitespacesAndNewlines)
                    if !name.isEmpty, let draft = model.stackStore.pane(for: paneKey)?.draft {
                        model.stackTemplateStore.savePrompt(promptTemplate(from: draft, name: name))
                    }
                    nameInput = ""
                }
                Button("Cancel", role: .cancel) { nameInput = "" }
            }

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
                GrammarChip(label: ";model", color: Konjo.stackViolet)
                GrammarChip(label: ";effort", color: Konjo.flame)
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
                    FacetPopoverContent(
                        paneKey: paneKey, card: model.stackStore.pane(for: paneKey)?.draft, isDraft: true
                    )
                    .presentationCompactAdaptation(.popover)
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

/// Prompt-scope templates menu — presets → saved prompt templates → save
/// current draft, matching macOS's `TemplatesMenuView` menu content (the
/// draft-only trigger path; the iOS composer never renders the committed-card
/// `CardbarButton` trigger, so that half of the macOS view isn't needed here).
private struct TemplatesMenuContent: View {
    let hot: Bool
    let prompts: [PromptTemplate]
    let onSelectPreset: (PresetKey) -> Void
    let onSelectPrompt: (PromptTemplate) -> Void
    let onSave: () -> Void

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                section("arrow.triangle.2.circlepath", "presets", Konjo.stackTeal) {
                    ForEach(PRESET_KEYS, id: \.self) { key in
                        row(name: ":\(PRESET_CATALOG[key]?.label ?? key.rawValue)", nameColor: Konjo.stackTeal,
                            desc: PRESET_DESCRIPTIONS[key]) { onSelectPreset(key) }
                    }
                }
                divider
                section("doc", "prompt templates", Konjo.sun) {
                    if prompts.isEmpty {
                        Text("none saved yet")
                            .font(Konjo.mono(10.5)).foregroundStyle(Konjo.fgMute)
                            .padding(.horizontal, 8).padding(.vertical, 4)
                    } else {
                        ForEach(prompts) { tpl in
                            row(name: tpl.name, desc: tpl.goal) { onSelectPrompt(tpl) }
                        }
                    }
                }
                divider
                section("square.and.arrow.down", "save", Konjo.fgDim) {
                    row(name: "save this prompt…", disabled: !hot, action: onSave)
                }
            }
            .padding(6)
        }
        .frame(width: 280).frame(maxHeight: 340)
        .background(Konjo.panel)
    }

    private var divider: some View {
        Divider().overlay(Konjo.line).padding(.vertical, 2)
    }

    private func section<Rows: View>(
        _ icon: String, _ text: String, _ color: Color, @ViewBuilder rows: () -> Rows
    ) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 6) {
                Image(systemName: icon).font(.system(size: 10))
                Text(text.uppercased()).font(Konjo.mono(8.5)).tracking(1)
            }
            .foregroundStyle(color)
            .padding(.horizontal, 8).padding(.vertical, 5)
            rows()
        }
    }

    private func row(
        name: String, nameColor: Color = Konjo.fg, desc: String? = nil,
        disabled: Bool = false, action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            VStack(alignment: .leading, spacing: 1) {
                Text(name).font(Konjo.mono(12)).foregroundStyle(disabled ? Konjo.fgMute : nameColor)
                if let desc {
                    Text(desc).font(Konjo.mono(10)).foregroundStyle(Konjo.fgDim).lineLimit(1)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 8).padding(.vertical, 6)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(disabled)
        .opacity(disabled ? 0.4 : 1)
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
                    FacetPopoverContent(paneKey: paneKey, card: card, isDraft: false)
                        .presentationCompactAdaptation(.popover)
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

// `FacetPopoverContent` and its six per-facet tabs now live in
// `FacetPopovers.swift` — real controls wired to `StackCard`/`StackConfig`,
// not the placeholder copy this file used to hold.
