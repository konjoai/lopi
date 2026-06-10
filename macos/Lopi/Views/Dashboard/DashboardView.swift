import SwiftUI

/// The "Forge" view — a parity port of the web dashboard's `AgentGrid`. A grid
/// of agent panes, each a live Forge orb, over the ambient starfield. A slim
/// stats strip floats above, mirroring the web top bar's live counters.
struct DashboardView: View {
    @EnvironmentObject private var model: AppModel

    private let columns = [GridItem(.adaptive(minimum: 240, maximum: 360), spacing: 14)]

    var body: some View {
        ZStack {
            KonjoBackground()
            if model.tasks.isEmpty {
                emptyState
            } else {
                ScrollView {
                    VStack(alignment: .leading, spacing: 16) {
                        statsStrip
                        grid
                    }
                    .padding(18)
                }
            }
        }
        .refreshable { await model.refreshAll() }
    }

    // MARK: Stats strip

    private var statsStrip: some View {
        HStack(spacing: 18) {
            stat("\(model.stats.running)", "running", Konjo.jade)
            stat("\(model.stats.queued)", "queued", Konjo.sun)
            stat("\(model.stats.succeeded)", "done", Konjo.jade.opacity(0.7))
            stat("\(model.stats.failed)", "failed", Konjo.rose)
            Spacer(minLength: 0)
            stat(String(format: "$%.2f", model.stats.totalCostUsdToday), "today", Konjo.flame)
        }
        .padding(.horizontal, 4)
    }

    private func stat(_ value: String, _ label: String, _ accent: Color) -> some View {
        HStack(spacing: 7) {
            Text(value)
                .font(Konjo.sans(18, weight: .semibold))
                .foregroundStyle(accent)
                .monospacedDigit()
            Text(label.uppercased())
                .font(Konjo.mono(9))
                .tracking(1.5)
                .foregroundStyle(Konjo.fgMute)
        }
    }

    // MARK: Grid

    private var grid: some View {
        LazyVGrid(columns: columns, spacing: 14) {
            ForEach(model.tasks.prefix(12)) { task in
                ForgePane(task: task)
                    .frame(height: 230)
            }
        }
    }

    // MARK: Empty

    private var emptyState: some View {
        VStack(spacing: 8) {
            ForgeOrb(phaseColor: Konjo.ice, activity: 0.25, pressure: 0.3, size: 120)
            Text("no agents")
                .font(Konjo.sans(20, weight: .bold))
                .foregroundStyle(Konjo.paper.opacity(0.35))
            Text("submit a goal to start a run")
                .font(Konjo.mono(10))
                .tracking(2)
                .foregroundStyle(Konjo.paper.opacity(0.25))
        }
    }
}
