import SwiftUI

/// StackTemplatesMenuView — the STACK-scope templates control
/// (Stack-Templates-1 §2b), in the dock's cardbar, icon-only, `Konjo.violet`,
/// immediately left of duplicate. Reuses the shared `CardbarButton` — no new
/// button style. Menu content is stack scope ONLY:
///
///   1. stack templates (violet) — drop the whole chain into this pane
///   2. saved stacks             — the other panes currently in `StackStore`;
///                                  picking one copies its cards into this pane
///   3. save this stack as template… (disabled when the pane has no cards)
///
/// No presets, no prompt templates — those live on each card
/// (`TemplatesMenuView`). "Saved stacks" is deliberately thin: nothing
/// persists a stack yet (`Persistence-1`), so this only ever lists panes
/// that are open right now, in this process.
struct StackTemplatesMenuView: View {
    var store: StackStore
    var templateStore: StackTemplateStore
    var paneKey: String
    var cards: [StackCard]

    @State private var open = false
    @State private var saveStackAlert = false
    @State private var nameInput = ""

    private var hasCards: Bool { !cards.isEmpty }
    private var otherPanes: [StackPaneState] { store.panes.filter { $0.key != paneKey } }

    var body: some View {
        CardbarButton(systemImage: "book", active: true, accent: Konjo.stackViolet, help: "stack templates") { open.toggle() }
            .popover(isPresented: $open, arrowEdge: .top) { menu }
            .alert("Name this stack template", isPresented: $saveStackAlert) {
                TextField("name", text: $nameInput)
                Button("Save") { commitSaveStack() }
                Button("Cancel", role: .cancel) { nameInput = "" }
            }
    }

    // MARK: Menu content

    private var menu: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                stacksSection
                sectionDivider
                savedSection
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

    private var stacksSection: some View {
        VStack(alignment: .leading, spacing: 0) {
            header("square.3.layers.3d", "stack templates", Konjo.stackViolet)
            if templateStore.library.stacks.isEmpty {
                emptyRow("none saved yet")
            } else {
                ForEach(templateStore.library.stacks) { tpl in
                    row(name: tpl.name, nameColor: Konjo.stackViolet,
                        desc: "\(tpl.loops.count) loop\(tpl.loops.count == 1 ? "" : "s")") {
                        store.applyStackTemplateToPane(paneKey, tpl)
                        open = false
                    }
                }
            }
        }
    }

    private var savedSection: some View {
        VStack(alignment: .leading, spacing: 0) {
            header("square.grid.2x2", "saved stacks", Konjo.fg)
            if otherPanes.isEmpty {
                emptyRow("no other open stacks")
            } else {
                ForEach(otherPanes) { pane in
                    row(name: pane.title, desc: "\(pane.cards.count) loop\(pane.cards.count == 1 ? "" : "s")") {
                        store.loadStackCardsIntoPane(paneKey, pane.key)
                        open = false
                    }
                }
            }
        }
    }

    private var saveSection: some View {
        VStack(alignment: .leading, spacing: 0) {
            header("square.and.arrow.down", "save", Konjo.fgDim)
            row(name: "save this stack…", disabled: !hasCards) {
                open = false; nameInput = ""; saveStackAlert = true
            }
        }
    }

    // MARK: Row primitives (duplicated from `TemplatesMenuView` — two small,
    // independently-scoped menus, exactly like the web's sibling components)

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

    private func emptyRow(_ text: String) -> some View {
        Text(text).font(Konjo.mono(10.5)).foregroundStyle(Konjo.fgMute)
            .padding(.horizontal, 8).padding(.vertical, 4)
    }

    // MARK: Save commit

    private func commitSaveStack() {
        let name = nameInput.trimmingCharacters(in: .whitespacesAndNewlines)
        if !name.isEmpty { templateStore.saveStack(stackTemplate(from: cards, name: name)) }
        nameInput = ""
    }
}
