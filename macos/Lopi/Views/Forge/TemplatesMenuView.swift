import SwiftUI

/// TemplatesMenuView — the PROMPT-scope templates control (Stack-Templates-1
/// §2a). Every card gets one: the draft renders it labeled (book symbol + the
/// word `templates` + a chevron) in its spec row — the teaching surface; a
/// committed card renders it icon-only via the shared `CardbarButton`,
/// `Konjo.sun`-accented, in its cardbar immediately left of duplicate. Same
/// view, `isDraft` just swaps the trigger.
///
/// Menu content is prompt scope ONLY — presets (teal) → prompt templates
/// (sun) → `save this prompt…`. Stack templates and "saved stacks" moved to
/// the stack-scope menu (`StackTemplatesMenuView`, in the dock) — a prompt
/// menu never offers a stack action.
///
/// Uses SwiftUI's `.popover` (the app's existing popover mechanism) so the
/// section colors match the web exactly; a native `Menu` can't tint
/// per-section text on macOS.
struct TemplatesMenuView: View {
    var store: StackStore
    var templateStore: StackTemplateStore
    var paneKey: String
    var card: StackCard
    /// True for the draft's labeled, teaching-surface trigger; false (the
    /// default) for a committed card's icon-only `CardbarButton` trigger.
    var isDraft = true

    @State private var open = false
    @State private var savePromptAlert = false
    @State private var nameInput = ""

    // A committed card always has a preset/goal already, so it's always
    // "hot" for enabling "save this prompt…"; the draft is only hot once it
    // carries enough to commit.
    private var hot: Bool { isDraft ? draftIsHot(card) : true }

    /// Route the patch to the right store op: the draft edits the pane's
    /// `draft`; a committed card edits itself in `pane.cards`.
    private func writeCard(_ mutate: (inout StackCard) -> Void) {
        if isDraft { store.updateDraftInPane(paneKey, mutate) }
        else { store.updateCardInPane(paneKey, card.id, mutate) }
    }

    var body: some View {
        Group {
            if isDraft {
                Button { open.toggle() } label: { labeledTrigger }
                    .buttonStyle(.plain)
            } else {
                CardbarButton(systemImage: "book", active: true, accent: Konjo.sun, help: "templates") { open.toggle() }
            }
        }
        .popover(isPresented: $open, arrowEdge: .bottom) { menu }
        .alert("Name this prompt template", isPresented: $savePromptAlert) {
            TextField("name", text: $nameInput)
            Button("Save") { commitSavePrompt() }
            Button("Cancel", role: .cancel) { nameInput = "" }
        }
    }

    // MARK: Trigger (draft only — committed cards use `CardbarButton` above)

    private var labeledTrigger: some View {
        HStack(spacing: 6) {
            Image(systemName: "book").font(.system(size: 12))
            Text("templates").font(Konjo.mono(11.5))
            Image(systemName: "chevron.down").font(.system(size: 8, weight: .bold))
        }
        .foregroundStyle(Konjo.fgDim)
        .padding(.horizontal, 10).frame(height: 29)
        .overlay(RoundedRectangle(cornerRadius: 7).stroke(Konjo.line2, lineWidth: 1))
        // No opaque background here (unlike `CardbarButton`) — without this,
        // only the icon/text glyphs are clickable, not the rest of the pill.
        .contentShape(Rectangle())
    }

    // MARK: Menu content

    private var menu: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                presetsSection
                sectionDivider
                promptsSection
                sectionDivider
                saveSection
            }
            .padding(6)
        }
        .frame(width: 300).frame(maxHeight: 360)
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
                    writeCard { $0 = applyPreset(key, to: $0) }
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
                        writeCard { $0 = applyPromptTemplate(tpl, to: $0) }
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

    // MARK: Save commit

    private func commitSavePrompt() {
        let name = nameInput.trimmingCharacters(in: .whitespacesAndNewlines)
        if !name.isEmpty { templateStore.savePrompt(promptTemplate(from: card, name: name)) }
        nameInput = ""
    }
}
