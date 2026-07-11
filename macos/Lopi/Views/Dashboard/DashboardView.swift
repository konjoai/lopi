import SwiftUI

/// The Forge — a live command center. The aurora backdrop breathes with fleet
/// activity; stat tiles roll; the cognition grid shows every running agent's
/// pressure/activity/throughput in real time; the ticker streams every event.
struct DashboardView: View {
    @Environment(AppModel.self) private var model

    private let statColumns = [GridItem(.adaptive(minimum: 132), spacing: 14)]
    private let agentColumns = [GridItem(.adaptive(minimum: 300), spacing: 16)]

    var body: some View {
        ZStack {
            AuroraBackground(intensity: model.aggregateActivity)
            ScrollView {
                VStack(alignment: .leading, spacing: 26) {
                    hero
                    statRow
                    if let breach = model.lastBudget { BudgetBanner(breach: breach) }
                    chartsRow
                    cognitionSection
                }
                .padding(28)
                .frame(maxWidth: 1320)
                .frame(maxWidth: .infinity)
            }
        }
        .refreshable { await model.refreshAll() }
    }

    // MARK: Hero

    private var hero: some View {
        HStack(alignment: .firstTextBaseline) {
            HStack(spacing: 12) {
                Text("FORGE")
                    .font(Konjo.sans(30, weight: .bold))
                    .foregroundStyle(
                        LinearGradient(colors: [Konjo.fg, Konjo.konjo2],
                                       startPoint: .leading, endPoint: .trailing)
                    )
                    .tracking(4)
                ConnectionLED(state: model.connection)
            }
            Spacer()
            if let v = model.serverVersion {
                Text("lopi \(v.version) · up \(Uptime.string(v.uptimeSecs))")
                    .font(Konjo.mono(10))
                    .foregroundStyle(Konjo.fgMute)
            }
        }
    }

    // MARK: Stats

    private var statRow: some View {
        LazyVGrid(columns: statColumns, spacing: 14) {
            LiveStat(label: "Running", value: Double(model.runningCount), accent: Konjo.konjo2,
                     pulse: model.runningCount > 0)
            LiveStat(label: "Queued", value: Double(model.queuedCount), accent: Konjo.fg)
            LiveStat(label: "Succeeded", value: Double(model.succeededCount), accent: Konjo.ok)
            LiveStat(label: "Failed", value: Double(model.failedCount), accent: Konjo.err)
            LiveStat(label: "Cost today", value: model.stats.totalCostUsdToday,
                     accent: Konjo.warn, format: "$%.2f")
        }
    }

    // MARK: Charts row

    private var chartsRow: some View {
        HStack(alignment: .top, spacing: 18) {
            budgetPanel
            tickerPanel
        }
    }

    private var budgetPanel: some View {
        VStack(alignment: .leading, spacing: 14) {
            SectionLabel(text: "Budget & Throughput")
            HStack(spacing: 22) {
                RadialGauge(fraction: budgetFraction, caption: "fleet / hr")
                VStack(alignment: .leading, spacing: 12) {
                    miniChart(title: "USD over session", samples: model.costSeries,
                              color: Konjo.warn,
                              value: String(format: "$%.4f", model.costSeries.last ?? 0))
                    miniChart(title: "tokens / sec", samples: model.throughputSeries,
                              color: Konjo.konjo2,
                              value: String(format: "%.0f", model.throughputSeries.last ?? 0))
                }
            }
        }
        .padding(20)
        .konjoSurface()
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func miniChart(title: String, samples: [Double], color: Color, value: String) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack {
                Text(title.uppercased())
                    .font(Konjo.mono(9)).foregroundStyle(Konjo.fgMute)
                Spacer()
                Text(value).font(Konjo.mono(10, weight: .medium)).foregroundStyle(color)
            }
            Sparkline(samples: samples, color: color)
                .frame(height: 40)
        }
    }

    private var tickerPanel: some View {
        VStack(alignment: .leading, spacing: 12) {
            SectionLabel(text: "Live Activity", trailing: "\(model.feed.count)")
            EventTicker(items: model.feed)
        }
        .padding(20)
        .konjoSurface()
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    // MARK: Cognition grid

    private var cognitionSection: some View {
        VStack(alignment: .leading, spacing: 14) {
            SectionLabel(text: "Agent Cognition",
                         trailing: "\(model.activeAgents.filter { $0.active }.count) active")
            if model.activeAgents.isEmpty {
                Text("No agents have reported yet. Submit a task to see live cognition.")
                    .font(Konjo.sans(13))
                    .foregroundStyle(Konjo.fgMute)
                    .padding(.vertical, 24)
            } else {
                LazyVGrid(columns: agentColumns, spacing: 16) {
                    ForEach(model.activeAgents) { agent in
                        CognitionCard(agent: agent)
                            .transition(.scale(scale: 0.92).combined(with: .opacity))
                    }
                }
                .animation(.spring(response: 0.45, dampingFraction: 0.85),
                           value: model.activeAgents.map(\.id))
            }
        }
    }

    private var budgetFraction: Double {
        min(model.stats.totalCostUsdToday / 25.0, 1.0)
    }
}

/// A glowing stat tile whose number rolls and which pulses while live.
struct LiveStat: View {
    let label: String
    let value: Double
    var accent: Color = Konjo.fg
    var format: String = "%.0f"
    var pulse: Bool = false

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(label.uppercased())
                .font(Konjo.mono(9, weight: .semibold))
                .foregroundStyle(Konjo.fgMute)
                .tracking(1)
            RollingNumber(value: value, format: format,
                          font: Konjo.sans(26, weight: .semibold), color: accent)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(16)
        .konjoSurface(12, fill: Konjo.bg1.opacity(0.65))
        .overlay(
            RoundedRectangle(cornerRadius: 12)
                .stroke(accent.opacity(pulse ? 0.4 : 0.0), lineWidth: 1)
        )
        .konjoGlow(accent, radius: 8, active: pulse)
    }
}

/// A pulsing banner shown briefly after a budget breach.
struct BudgetBanner: View {
    let breach: BudgetBreach
    @State private var glow = false

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: "dollarsign.circle.fill").foregroundStyle(Konjo.warn)
            Text("Budget exceeded — \(breach.scope)")
                .font(Konjo.sans(13, weight: .semibold)).foregroundStyle(Konjo.fg)
            Spacer()
            Text(String(format: "$%.2f / $%.2f", breach.burnedUsd, breach.limitUsd))
                .font(Konjo.mono(12)).foregroundStyle(Konjo.warn)
        }
        .padding(14)
        .konjoSurface(10, fill: Konjo.warn.opacity(0.10))
        .konjoGlow(Konjo.warn, radius: glow ? 14 : 6, active: true)
        .onAppear {
            withAnimation(.easeInOut(duration: 1.1).repeatForever(autoreverses: true)) {
                glow = true
            }
        }
    }
}

/// Human-readable uptime ("3h 12m", "45s").
enum Uptime {
    static func string(_ secs: Int) -> String {
        if secs >= 3600 { return "\(secs / 3600)h \((secs % 3600) / 60)m" }
        if secs >= 60 { return "\(secs / 60)m" }
        return "\(secs)s"
    }
}
