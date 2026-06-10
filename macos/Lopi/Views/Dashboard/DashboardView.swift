import SwiftUI

/// The "Forge" parity screen: live stats strip, agent grid, budget gauge, and a
/// rolling log tail — all driven by the `/ws` stream via `AppModel`.
struct DashboardView: View {
    @Environment(AppModel.self) private var model

    private let columns = [GridItem(.adaptive(minimum: 280), spacing: 16)]

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 28) {
                statsStrip
                budgetRow
                agentGrid
                logPanel
            }
            .padding(28)
        }
        .background(Konjo.bg)
        .refreshable { await model.refreshAll() }
    }

    private var statsStrip: some View {
        LazyVGrid(columns: [GridItem(.adaptive(minimum: 150), spacing: 12)], spacing: 12) {
            StatCard(label: "Running", value: "\(model.stats.running)", accent: Konjo.konjo2)
            StatCard(label: "Queued", value: "\(model.stats.queued)", accent: Konjo.fg)
            StatCard(label: "Succeeded", value: "\(model.stats.succeeded)", accent: Konjo.ok)
            StatCard(label: "Failed", value: "\(model.stats.failed)", accent: Konjo.err)
            StatCard(label: "Cost today", value: String(format: "$%.2f", model.stats.totalCostUsdToday), accent: Konjo.warn)
        }
    }

    private var budgetRow: some View {
        KonjoPanel {
            HStack(spacing: 28) {
                RadialGauge(fraction: budgetFraction, caption: "fleet / hr")
                VStack(alignment: .leading, spacing: 8) {
                    Text("Budget")
                        .font(Konjo.sans(16, weight: .semibold))
                        .foregroundStyle(Konjo.fg)
                    Text(String(format: "$%.2f spent today", model.stats.totalCostUsdToday))
                        .font(Konjo.mono(12))
                        .foregroundStyle(Konjo.fgDim)
                    Text("\(model.stats.totalTokensToday) tokens today")
                        .font(Konjo.mono(12))
                        .foregroundStyle(Konjo.fgMute)
                }
                Spacer()
            }
        }
    }

    /// Cost relative to the fleet's $25/hr cap (matches the web breaker scope).
    private var budgetFraction: Double {
        min(model.stats.totalCostUsdToday / 25.0, 1.0)
    }

    private var agentGrid: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("AGENTS")
                .font(Konjo.mono(11))
                .foregroundStyle(Konjo.fgMute)
            if model.tasks.isEmpty {
                Text("No tasks yet")
                    .font(Konjo.sans(13))
                    .foregroundStyle(Konjo.fgMute)
            } else {
                LazyVGrid(columns: columns, spacing: 16) {
                    ForEach(model.tasks.prefix(12)) { task in
                        agentCard(task)
                    }
                }
            }
        }
    }

    private func agentCard(_ task: TaskSummary) -> some View {
        KonjoPanel {
            VStack(alignment: .leading, spacing: 10) {
                HStack {
                    StatusOrb(status: task.status)
                    Text(task.status)
                        .font(Konjo.mono(11))
                        .foregroundStyle(Konjo.fgDim)
                    Spacer()
                    Text(task.id.prefix(8))
                        .font(Konjo.mono(10))
                        .foregroundStyle(Konjo.fgMute)
                }
                Text(task.goal)
                    .font(Konjo.sans(13))
                    .foregroundStyle(Konjo.fg)
                    .lineLimit(2)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private var logPanel: some View {
        KonjoPanel {
            VStack(alignment: .leading, spacing: 8) {
                Text("LOG STREAM")
                    .font(Konjo.mono(11))
                    .foregroundStyle(Konjo.fgMute)
                if model.recentLogs.isEmpty {
                    Text("Waiting for live events…")
                        .font(Konjo.mono(11))
                        .foregroundStyle(Konjo.fgMute)
                } else {
                    ForEach(Array(model.recentLogs.suffix(12).enumerated()), id: \.offset) { _, line in
                        Text(line)
                            .font(Konjo.mono(11))
                            .foregroundStyle(Konjo.fgDim)
                            .lineLimit(1)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
            }
        }
    }
}
