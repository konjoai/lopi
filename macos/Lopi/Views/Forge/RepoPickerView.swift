import SwiftUI
import LopiStacksKit

/// The repo field's control: the same `ConfigChip` the other four config fields
/// wear, opening a searchable, owner-grouped popover.
///
/// It is not a `StackDropdown` because a native `Menu` cannot host a text field,
/// and past a hundred-odd repos a flat unsearchable list is unusable. So only
/// the *presentation* forks — the chip is the shared view, and the section /
/// filter / order rules are the same pure `RepoMenu` code the web runs, pinned
/// to one golden fixture.
///
/// Modelled on `TemplatesMenuView`, the app's other sectioned `.popover`:
/// `Button` trigger → `.popover(isPresented:arrowEdge:)` → a bounded `ScrollView`
/// over `Konjo.panel`.
struct RepoPickerView: View {
    var label: String
    var value: String
    var options: [StackOption]
    var accent: Color = Konjo.sun
    var onSelect: (String) -> Void

    @State private var open = false
    @State private var query = ""
    @FocusState private var searchFocused: Bool

    /// What the chip reads. Falls back to the raw value so a repo the server has
    /// stopped listing still shows what the card is actually set to.
    private var currentLabel: String {
        options.first { $0.value == value }?.label ?? (value.isEmpty ? "auto" : value)
    }

    private var menu: OptionMenu { groupedMenu(options, query: query) }

    var body: some View {
        Button { open.toggle() } label: {
            ConfigChip(label: label, text: currentLabel, icon: "folder", accent: accent)
        }
        .buttonStyle(.plain)
        .popover(isPresented: $open, arrowEdge: .bottom) { panel }
    }

    private var panel: some View {
        VStack(alignment: .leading, spacing: 0) {
            searchField
            Rectangle().fill(Konjo.line).frame(height: 1)
            list
        }
        .frame(width: 340)
        .background(Konjo.panel)
        // Focus on open, so the picker is type-to-filter without a click. The
        // popover's content is built before it is on screen; focusing in the
        // same runloop turn is dropped.
        .onAppear {
            query = ""
            DispatchQueue.main.async { searchFocused = true }
        }
    }

    private var searchField: some View {
        TextField("search repos…", text: $query)
            .konjoField(focused: searchFocused, accent: accent)
            .focused($searchFocused)
            // Escape clears a live query first and only then closes — one
            // keystroke should not throw away the filter *and* the popover.
            .onExitCommand { if query.isEmpty { open = false } else { query = "" } }
            .padding(8)
    }

    private var list: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 0) {
                ForEach(menu.pinned) { row in
                    optionRow(row.opt)
                }
                ForEach(menu.groups) { group in
                    sectionHeader(group.key)
                    ForEach(group.rows) { row in
                        optionRow(row.opt)
                    }
                }
                if menu.flat.isEmpty {
                    Text("no match").font(Konjo.mono(10.5)).foregroundStyle(Konjo.fgMute)
                        .padding(.horizontal, 8).padding(.vertical, 6)
                }
            }
            .padding(6)
        }
        .frame(maxHeight: 440)
    }

    private func sectionHeader(_ key: String) -> some View {
        Text(key.uppercased()).font(Konjo.mono(8.5)).tracking(1)
            .foregroundStyle(accent)
            .padding(.horizontal, 8).padding(.top, 8).padding(.bottom, 3)
    }

    private func optionRow(_ opt: StackOption) -> some View {
        Button {
            onSelect(opt.value)
            open = false
        } label: {
            // Path stacked under the label, not beside it — a full path inline
            // would demand roughly double the width. Matches `TemplatesMenuView`'s
            // name-over-desc row, and web's `.kdrop-item.stacked`.
            VStack(alignment: .leading, spacing: 1) {
                Text(opt.label)
                    .font(Konjo.mono(12))
                    .foregroundStyle(opt.value == value ? accent : Konjo.fg)
                    .lineLimit(1)
                if !opt.hint.isEmpty {
                    // Truncate at the head: the tail
                    // (`…/squish-w100-hf-url-normalize`) is what tells two
                    // checkouts of one repo apart, so the end must survive.
                    Text(opt.hint).font(Konjo.mono(9)).foregroundStyle(Konjo.fgMute)
                        .lineLimit(1).truncationMode(.head)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 8).padding(.vertical, 5)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}
