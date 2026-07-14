import SwiftUI

/// TemplatesMenuView — the draft card's single sectioned templates control
/// (Creation-Flow-1 §5). One trigger (book symbol + the word `templates` + a
/// chevron, no other label) opens one color-coded popover with four sections:
///   1. presets (teal)          — the five PRESET_CATALOG presets
///   2. prompt templates (sun)  — fill the draft (preset + goal + provenance)
///   3. stack templates (violet)— drop the whole chain into the pane at once
///   4. save (dim)              — save this prompt… / save this stack…
///
/// Uses SwiftUI's `.popover` (the app's existing popover mechanism — not a
/// forked second system) so the section colors match the web exactly; a native
/// `Menu` can't tint per-section text on macOS. Name prompts use native alerts.
struct TemplatesMenuView: View {
    var store: StackStore
    var templateStore: StackTemplateStore
    var paneKey: String
    var draft: StackCard
    var paneCards: [StackCard]

    @State private var open = false
    @State private var savePromptAlert = false
    @State private var saveStackAlert = false
    @State private var nameInput = ""

    private var hot: Bool { draftIsHot(draft) }
    private var hasCards: Bool { !paneCards.isEmpty }

    var body: some View {
        Button { open.toggle() } label: { trigger }
            .buttonStyle(.plain)
            .popover(isPresented: $open, arrowEdge: .bottom) { menu }
            .alert("Name this prompt template", isPresented: $savePromptAlert) {
                TextField("name", text: $nameInput)
                Button("Save") { commitSavePrompt() }
                Button("Cancel", role: .cancel) { nameInput = "" }
            }
            .alert("Name this stack template", isPresented: $saveStackAlert) {
                TextField("name", text: $nameInput)
                Button("Save") { commitSaveStack() }
                Button("Cancel", role: .cancel) { nameInput = "" }
            }
    }

    // MARK: Trigger

    private var trigger: some View {
        HStack(spacing: 6) {
            Image(systemName: "book").font(.system(size: 12))
            Text("templates").font(Konjo.mono(11.5))
            Image(systemName: "chevron.down").font(.system(size: 8, weight: .bold))
        }
        .foregroundStyle(Konjo.fgDim)
        .padding(.horizontal, 10).frame(height: 29)
        .overlay(RoundedRectangle(cornerRadius: 7).stroke(Konjo.line2, lineWidth: 1))
    }

    // MARK: Menu content

    private var menu: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                presetsSection
                sectionDivider
                promptsSection
                sectionDivider
                stacksSection
                sectionDivider
                saveSection
            }
            .padding(6)
        }
        .frame(width: 300).frame(maxHeight: 440)
        .background(Konjo.panel)
    }

    private var sectionDivider: some View {
        Divider().overlay(Konjo.line).padding(.vertical, 2)
    }

    private var presetsSection: some View {
        VStack(alignment: .leading, spacing: 0) {
            header("arrow.triangle.2.circlepath", "presets", Konjo.stackTeal)
            ForEach(PRESET_KEYS, id: \.self) { key in
                row(name: ":\(PRESET_CATALOG[key]?.label ?? key.rawValue)", nameColor: Konjo.stackTeal,
                    desc: PRESET_DESCRIPTIONS[key]) {
                    store.updateDraftInPane(paneKey) { $0 = applyPreset(key, to: $0) }
                    open = false
                }
            }
        }
    }

    private var promptsSection: some View {
        VStack(alignment: .leading, spacing: 0) {
            header("doc", "prompt templates", Konjo.sun)
            if templateStore.library.prompts.isEmpty {
                emptyRow
            } else {
                ForEach(templateStore.library.prompts) { tpl in
                    row(name: tpl.name, desc: tpl.goal) {
                        store.updateDraftInPane(paneKey) { $0 = applyPromptTemplate(tpl, to: $0) }
                        open = false
                    }
                }
            }
        }
    }

    private var stacksSection: some View {
        VStack(alignment: .leading, spacing: 0) {
            header("square.3.layers.3d", "stack templates", Konjo.stackViolet)
            if templateStore.library.stacks.isEmpty {
                emptyRow
            } else {
                ForEach(templateStore.library.stacks) { tpl in
                    row(name: tpl.name, desc: "\(tpl.loops.count) loop\(tpl.loops.count == 1 ? "" : "s")") {
                        store.applyStackTemplateToPane(paneKey, tpl)
                        open = false
                    }
                }
            }
        }
    }

    private var saveSection: some View {
        VStack(alignment: .leading, spacing: 0) {
            header("square.and.arrow.down", "save", Konjo.fgDim)
            row(name: "save this prompt…", disabled: !hot) {
                open = false; nameInput = ""; savePromptAlert = true
            }
            row(name: "save this stack…", disabled: !hasCards) {
                open = false; nameInput = ""; saveStackAlert = true
            }
        }
    }

    // MARK: Row primitives

    private func header(_ icon: String, _ text: String, _ color: Color) -> some View {
        HStack(spacing: 6) {
            Image(systemName: icon).font(.system(size: 10))
            Text(text.uppercased()).font(Konjo.mono(8.5)).tracking(1)
        }
        .foregroundStyle(color)
        .padding(.horizontal, 8).padding(.vertical, 5)
    }

    private func row(name: String, nameColor: Color = Konjo.fg, desc: String? = nil,
                     disabled: Bool = false, action: @escaping () -> Void) -> some View {
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

    private var emptyRow: some View {
        Text("none saved yet").font(Konjo.mono(10.5)).foregroundStyle(Konjo.fgMute)
            .padding(.horizontal, 8).padding(.vertical, 4)
    }

    // MARK: Save commits

    private func commitSavePrompt() {
        let name = nameInput.trimmingCharacters(in: .whitespacesAndNewlines)
        if !name.isEmpty { templateStore.savePrompt(promptTemplate(from: draft, name: name)) }
        nameInput = ""
    }

    private func commitSaveStack() {
        let name = nameInput.trimmingCharacters(in: .whitespacesAndNewlines)
        if !name.isEmpty { templateStore.saveStack(stackTemplate(from: paneCards, name: name)) }
        nameInput = ""
    }
}
