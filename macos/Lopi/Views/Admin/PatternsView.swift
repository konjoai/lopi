import SwiftUI

/// Mined success patterns: keyword clusters with their observed success rate
/// and average attempts, from the SQLite pattern store.
struct PatternsView: View {
    @Environment(AppModel.self) private var model
    @State private var rows: [PatternModel] = []
    @State private var loaded = false

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                if loaded && rows.isEmpty {
                    EmptyHint(icon: "sparkles", text: "No patterns mined yet — they appear as tasks complete.")
                }
                ForEach(rows) { row in
                    card(row)
                }
            }
            .padding(20)
        }
        .background(Konjo.bg)
        .task {
            rows = await model.patterns()
            loaded = true
        }
    }

    private func card(_ row: PatternModel) -> some View {
        KonjoPanel {
            VStack(alignment: .leading, spacing: 8) {
                Text(row.goalKeywords)
                    .font(Konjo.mono(12))
                    .foregroundStyle(Konjo.konjo2)
                HStack(spacing: 16) {
                    if let rate = row.successRate {
                        successBar(rate)
                    }
                    if let attempts = row.avgAttempts {
                        Text(String(format: "%.1f avg attempts", attempts))
                            .font(Konjo.mono(10))
                            .foregroundStyle(Konjo.fgDim)
                    }
                    Spacer()
                    Text("seen \(DateFormatting.short(row.lastSeen))")
                        .font(Konjo.mono(10))
                        .foregroundStyle(Konjo.fgMute)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private func successBar(_ rate: Double) -> some View {
        HStack(spacing: 8) {
            GeometryReader { geo in
                ZStack(alignment: .leading) {
                    Capsule().fill(Konjo.line2)
                    Capsule()
                        .fill(rate >= 0.7 ? Konjo.ok : (rate >= 0.4 ? Konjo.warn : Konjo.err))
                        .frame(width: geo.size.width * min(max(rate, 0), 1))
                }
            }
            .frame(width: 120, height: 6)
            Text("\(Int(rate * 100))%")
                .font(Konjo.mono(10))
                .foregroundStyle(Konjo.fgDim)
        }
    }
}
