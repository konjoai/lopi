import SwiftUI

/// Fleet health rollup from the heartbeat registry: healthy / degraded / dead
/// counts. Per-agent detail arrives once agents emit heartbeats.
struct HealthView: View {
    @Environment(AppModel.self) private var model
    @State private var summary: HealthSummary?

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                if let s = summary {
                    LazyVGrid(columns: [GridItem(.adaptive(minimum: 150), spacing: 12)], spacing: 12) {
                        StatCard(label: "Tracked", value: "\(s.total)", accent: Konjo.fg)
                        StatCard(label: "Healthy", value: "\(s.healthy)", accent: Konjo.ok)
                        StatCard(label: "Degraded", value: "\(s.degraded)", accent: Konjo.warn)
                        StatCard(label: "Dead", value: "\(s.dead)", accent: Konjo.err)
                    }
                    if s.total == 0 {
                        EmptyHint(
                            icon: "heart.text.square",
                            text: "No agents have sent heartbeats yet. Heartbeats register via POST /api/agents/:id/heartbeat."
                        )
                    }
                }
            }
            .padding(20)
        }
        .background(Konjo.bg)
        .task { summary = await model.healthSummary() }
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button {
                    Task { summary = await model.healthSummary() }
                } label: {
                    Label("Refresh", systemImage: "arrow.clockwise")
                }
            }
        }
    }
}
