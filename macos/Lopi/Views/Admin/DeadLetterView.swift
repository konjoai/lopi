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
    var accent: Color = Konjo.ice

    @State private var breathe = false

    var body: some View {
        VStack(spacing: 16) {
            // Accent-tinted icon badge with a soft breathing halo.
            ZStack {
                Circle()
                    .fill(accent.opacity(0.10))
                    .frame(width: 72, height: 72)
                    .scaleEffect(breathe ? 1.06 : 0.94)
                    .opacity(breathe ? 1 : 0.7)
                Circle()
                    .stroke(accent.opacity(0.25), lineWidth: 1)
                    .frame(width: 72, height: 72)
                Image(systemName: icon)
                    .font(.system(size: 26, weight: .light))
                    .foregroundStyle(accent.opacity(0.8))
            }
            .shadow(color: accent.opacity(0.25), radius: 18)
            Text(text)
                .font(Konjo.sans(13))
                .foregroundStyle(Konjo.fgDim)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 360)
                .lineSpacing(2)
        }
        .padding(.vertical, 56)
        .frame(maxWidth: .infinity)
        .onAppear {
            withAnimation(.easeInOut(duration: 2.4).repeatForever(autoreverses: true)) {
                breathe = true
            }
        }
    }
}
