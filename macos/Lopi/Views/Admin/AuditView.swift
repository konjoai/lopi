import SwiftUI

/// Append-only audit log with action filtering and cursor pagination.
struct AuditView: View {
    @Environment(AppModel.self) private var model
    @State private var entries: [AuditEntry] = []
    @State private var cursor = 0
    @State private var actionFilter = ""
    @State private var loaded = false
    @State private var exhausted = false

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 10) {
                filterBar
                if loaded && entries.isEmpty {
                    EmptyHint(icon: "doc.text.magnifyingglass", text: "No audit events match.")
                }
                ForEach(entries) { entry in
                    row(entry)
                }
                if !entries.isEmpty && !exhausted {
                    Button("Load more") { Task { await loadPage() } }
                        .frame(maxWidth: .infinity)
                        .padding(.top, 6)
                }
            }
            .padding(20)
        }
        .background(Konjo.bg)
        .task { await reload() }
    }

    private var filterBar: some View {
        HStack {
            TextField("Filter by action (e.g. task.dispatch)", text: $actionFilter)
                .konjoField()
                .onSubmit { Task { await reload() } }
            Button("Apply") { Task { await reload() } }
                .konjoButton()
        }
    }

    private func row(_ entry: AuditEntry) -> some View {
        KonjoPanel {
            VStack(alignment: .leading, spacing: 6) {
                HStack {
                    Text(entry.action)
                        .font(Konjo.mono(11, weight: .semibold))
                        .foregroundStyle(Konjo.konjo2)
                    if let st = entry.subjectType, let sid = entry.subjectId {
                        Text("\(st):\(sid.prefix(8))")
                            .font(Konjo.mono(10))
                            .foregroundStyle(Konjo.fgDim)
                    }
                    Spacer()
                    if let actor = entry.actor {
                        Text(actor)
                            .font(Konjo.mono(10))
                            .foregroundStyle(Konjo.fgMute)
                    }
                    Text(DateFormatting.short(entry.ts))
                        .font(Konjo.mono(10))
                        .foregroundStyle(Konjo.fgMute)
                }
                if let payload = entry.payload, !payload.isEmpty {
                    Text(payload)
                        .font(Konjo.mono(10))
                        .foregroundStyle(Konjo.fgDim)
                        .lineLimit(2)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private func reload() async {
        entries = []
        cursor = 0
        exhausted = false
        await loadPage()
        loaded = true
    }

    private func loadPage() async {
        let filter = actionFilter.trimmingCharacters(in: .whitespaces)
        let page = await model.audit(sinceId: cursor, action: filter.isEmpty ? nil : filter)
        entries.append(contentsOf: page.entries)
        exhausted = page.entries.isEmpty || page.nextCursor == cursor
        cursor = page.nextCursor
    }
}
