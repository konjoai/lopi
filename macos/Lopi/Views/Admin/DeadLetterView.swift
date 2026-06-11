import SwiftUI

/// Dead-letter queue: tasks that exhausted their retry budget. Operators can
/// inspect the final error, re-queue, or permanently discard each row.
struct DeadLetterView: View {
    @Environment(AppModel.self) private var model
    @State private var rows: [DeadLetter] = []
    @State private var loaded = false

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                if loaded && rows.isEmpty {
                    EmptyHint(icon: "tray", text: "Dead-letter queue is empty — no tasks have exhausted their retries.")
                }
                ForEach(rows) { row in
                    card(row)
                }
            }
            .padding(20)
        }
        .background(Konjo.bg)
        .task { await reload() }
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button { Task { await reload() } } label: {
                    Label("Refresh", systemImage: "arrow.clockwise")
                }
            }
        }
    }

    private func card(_ row: DeadLetter) -> some View {
        KonjoPanel {
            VStack(alignment: .leading, spacing: 8) {
                HStack {
                    Text(row.goal)
                        .font(Konjo.sans(14, weight: .semibold))
                        .foregroundStyle(Konjo.fg)
                        .lineLimit(2)
                    Spacer()
                    Text("\(row.totalAttempts) attempts")
                        .font(Konjo.mono(10))
                        .foregroundStyle(Konjo.warn)
                }
                if let error = row.lastError {
                    Text(error)
                        .font(Konjo.mono(11))
                        .foregroundStyle(Konjo.err)
                        .lineLimit(3)
                }
                HStack(spacing: 14) {
                    Text("died \(DateFormatting.short(row.deadAt))")
                    Text("source: \(row.source)")
                    if let repo = row.repoPath {
                        Text(repo).lineLimit(1)
                    }
                    Spacer()
                    Button {
                        Task {
                            if await model.retryDeadLetter(row.id) { await reload() }
                        }
                    } label: {
                        Label("Retry", systemImage: "arrow.uturn.up")
                    }
                    Button(role: .destructive) {
                        Task {
                            if await model.discardDeadLetter(row.id) { await reload() }
                        }
                    } label: {
                        Label("Discard", systemImage: "trash")
                    }
                }
                .font(Konjo.mono(10))
                .foregroundStyle(Konjo.fgMute)
                .buttonStyle(.borderless)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private func reload() async {
        rows = await model.deadLetters()
        loaded = true
    }
}

/// Shared empty-state hint for admin lists.
struct EmptyHint: View {
    let icon: String
    let text: String

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: icon)
                .foregroundStyle(Konjo.fgMute)
            Text(text)
                .font(Konjo.sans(13))
                .foregroundStyle(Konjo.fgMute)
        }
        .padding(.top, 30)
        .frame(maxWidth: .infinity)
    }
}
