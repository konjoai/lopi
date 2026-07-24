import SwiftUI
import LopiStacksKit

/// Budget — cost governance. Live fleet spend, burn-rate vs a configurable
/// hourly cap, projection, per-agent spend with stop controls, and a fleet
/// kill switch. The macOS counterpart of the web Budget view (Phase 10).
struct BudgetView: View {
    @Environment(AppModel.self) private var model
    @State private var cap: Double = BudgetView.loadCap()
    @State private var alertPct: Double = BudgetView.loadAlertPct()
    @State private var samples: [(t: Date, cost: Double)] = []
    /// Cost-by-model + 7-day trend, polled from `GET /api/budget/breakdown`
    /// while this view is on screen (mirrors web's `startBudgetBreakdownPoller`).
    @State private var breakdown = BudgetBreakdown()

    private let tick = Timer.publish(every: 2, on: .main, in: .common).autoconnect()
    private let presets: [Double] = [1, 5, 10, 25, 50]

    // ── Live rollup ──────────────────────────────────────────────────────────
    private var spent: Double { model.liveAgents.values.reduce(0) { $0 + $1.costUsd } }
    private var running: Int { model.liveAgents.values.filter { $0.active }.count }
    private var burnPerHour: Double {
        guard samples.count >= 2, let f = samples.first, let l = samples.last else { return 0 }
        let dt = l.t.timeIntervalSince(f.t)
        return dt > 0 ? max(0, (l.cost - f.cost) / dt * 3600) : 0
    }
    private var fraction: Double { cap > 0 ? burnPerHour / cap : 0 }
    private var state: BudgetState { fraction >= 1 ? .over : fraction >= 0.75 ? .warn : .ok }
    private var color: Color {
        switch state { case .over: return Konjo.rose; case .warn: return Konjo.flame; case .ok: return Konjo.jade }
    }
    private var minutesToCap: Double? {
        let rem = cap - spent
        return burnPerHour > 0 && rem > 0 ? rem / burnPerHour * 60 : nil
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                header
                statCards
                spendTrend
                burnMeter
                byRepoBreakdown
                byModelBreakdown
                topSpenders
                if !model.budgetBreaches.isEmpty { breachHistory }
            }
            .padding(20)
            .frame(maxWidth: 920, alignment: .leading)
            .frame(maxWidth: .infinity)
        }
        .background(Konjo.bg)
        .onReceive(tick) { _ in
            samples.append((Date(), spent))
            if samples.count > 150 { samples.removeFirst(samples.count - 150) }
        }
        .task { await pollBreakdown() }
    }

    /// Poll the durable cost-breakdown endpoint while this view is mounted.
    /// `.task` auto-cancels on disappear, so there's no separate teardown —
    /// mirrors web's view-scoped `onMount { startBudgetBreakdownPoller() }`.
    private func pollBreakdown() async {
        while !Task.isCancelled {
            if let fresh = try? await model.client.budgetBreakdown() {
                breakdown = fresh
            }
            try? await Task.sleep(nanoseconds: 15_000_000_000)
        }
    }

    // MARK: Header

    private var header: some View {
        HStack(alignment: .firstTextBaseline) {
            VStack(alignment: .leading, spacing: 2) {
                Text("Budget").font(Konjo.sans(22, weight: .semibold)).foregroundStyle(Konjo.fg)
                Text("COST GOVERNANCE · LIVE BURN VS CAP")
                    .font(Konjo.mono(9, weight: .semibold)).tracking(1.4).foregroundStyle(Konjo.fgMute)
            }
            Spacer()
            Button { stopAll() } label: {
                Label("Stop all running (\(running))", systemImage: "stop.fill")
            }
            .konjoButton(Konjo.rose)
            .disabled(running == 0)
            .opacity(running == 0 ? 0.4 : 1)
        }
    }

    // MARK: Stat cards

    private var statCards: some View {
        HStack(spacing: 12) {
            stat("SPENT (SESSION)", String(format: "$%.4f", spent), Konjo.flame)
            stat("BURN RATE", String(format: "$%.2f/h", burnPerHour), color)
            stat("HOURLY CAP", String(format: "$%.2f", cap), Konjo.ice)
            stat("TO CAP", fmtMins(minutesToCap), color)
            stat("TOKENS", tokensDisplay, Konjo.sun)
            stat("RUNNING", "\(running)", Konjo.jade)
        }
    }

    /// Total tokens billed today (UTC), from the same `/api/stats` poll the
    /// Dashboard's COST TODAY tile reads — abbreviated past 1000, matching
    /// web's `tokensDisplay`.
    private var tokensDisplay: String {
        let tokens = model.stats.totalTokensToday
        return tokens >= 1000 ? "\(Int((Double(tokens) / 1000).rounded()))K" : "\(tokens)"
    }

    private func stat(_ label: String, _ value: String, _ c: Color) -> some View {
        VStack(alignment: .leading, spacing: 5) {
            Text(label).font(Konjo.mono(9)).tracking(0.8).foregroundStyle(Konjo.fgMute)
            Text(value).font(Konjo.sans(20, weight: .semibold)).foregroundStyle(c).monospacedDigit()
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(14)
        .konjoSurface(12)
    }

    // MARK: Burn meter + cap setter

    private var burnMeter: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Text("BURN VS CAP").font(Konjo.mono(9, weight: .semibold)).tracking(1).foregroundStyle(Konjo.fgDim)
                Spacer()
                Text("\(Int(fraction * 100))% of cap").font(Konjo.mono(11)).foregroundStyle(color).monospacedDigit()
            }
            GeometryReader { g in
                ZStack(alignment: .leading) {
                    Capsule().fill(Color.black.opacity(0.4))
                    Capsule().fill(color)
                        .frame(width: g.size.width * CGFloat(min(1, fraction)))
                        .shadow(color: color.opacity(0.7), radius: 8)
                    Rectangle().fill(Color.white.opacity(0.3)).frame(width: 1)
                        .offset(x: g.size.width * 0.75)
                }
            }
            .frame(height: 12)
            .animation(.easeOut(duration: 0.4), value: fraction)

            HStack(spacing: 8) {
                Text("CAP $/H").font(Konjo.mono(9)).tracking(0.8).foregroundStyle(Konjo.fgMute)
                ForEach(presets, id: \.self) { p in
                    Button { setCap(p) } label: { Text("$\(Int(p))") }
                        .konjoButton(cap == p ? Konjo.ice : Konjo.fgDim)
                }
                Stepper("", value: Binding(get: { cap }, set: { setCap($0) }), in: 0.5...200, step: 0.5)
                    .labelsHidden()
            }

            HStack(spacing: 10) {
                Text("ALERT THRESHOLD").font(Konjo.mono(9.5)).tracking(0.8).foregroundStyle(Konjo.fgMute)
                Slider(value: Binding(get: { alertPct }, set: { setAlertPct($0) }), in: 10...100, step: 1)
                Text("\(Int(alertPct))%").font(Konjo.mono(10.5)).foregroundStyle(Konjo.flame)
                    .frame(width: 34, alignment: .trailing)
            }
        }
        .padding(16)
        .konjoSurface(12)
    }

    // MARK: Spend trend (7 days)

    private var spendTrend: some View {
        let bars = trendBars(breakdown.trend)
        let delta = trendDelta(breakdown.trend)
        return VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text("SPEND, LAST 7 DAYS").font(Konjo.mono(9, weight: .semibold)).tracking(1).foregroundStyle(Konjo.fgDim)
                Spacer()
                if let delta {
                    HStack(spacing: 3) {
                        Text(delta.up ? "▲" : "▼")
                        Text(delta.pct.map { "\($0)%" } ?? "new spend")
                        Text("vs 6-day avg")
                    }
                    .font(Konjo.mono(11))
                    .foregroundStyle(delta.up ? Konjo.jade : Konjo.flame)
                }
            }
            if bars.isEmpty {
                Text("no spend recorded yet").font(Konjo.mono(11)).foregroundStyle(Konjo.fgMute)
                    .frame(maxWidth: .infinity).padding(.vertical, 10)
            } else {
                HStack(alignment: .bottom, spacing: 6) {
                    ForEach(Array(bars.enumerated()), id: \.offset) { _, bar in
                        RoundedRectangle(cornerRadius: 2)
                            .fill(bar.isToday ? Konjo.stackTeal : Konjo.stackTeal.opacity(0.35))
                            .frame(height: max(2, 64 * bar.heightPct / 100))
                    }
                }
                .frame(height: 64, alignment: .bottom)
                HStack(spacing: 6) {
                    ForEach(Array(bars.enumerated()), id: \.offset) { _, bar in
                        Text(bar.label).frame(maxWidth: .infinity)
                    }
                }
                .font(Konjo.mono(9)).foregroundStyle(Konjo.fgMute)
            }
        }
        .padding(16)
        .konjoSurface(12)
    }

    // MARK: By-repo breakdown

    /// Cost grouped by repo, from the live session's agent map — client-side,
    /// same scope as `spent`/`topSpenders` above. Was blocked until
    /// macOS-Web-Parity-5 threaded `repo` onto `LiveAgent`/
    /// `AgentEvent.taskStarted`; see that sprint's `LEDGER.md` entry for why
    /// it wasn't buildable before. Grouping logic lives in
    /// `groupCostByRepo` (`Store/BudgetRepoBreakdown.swift`) so it's
    /// unit-testable without a live view.
    private var byRepoBreakdown: some View {
        let repos = groupCostByRepo(model.liveAgents)
        let maxCost = max(1, repos.map(\.cost).max() ?? 0)
        return VStack(alignment: .leading, spacing: 10) {
            Text("BY REPO").font(Konjo.mono(9, weight: .semibold)).tracking(1).foregroundStyle(Konjo.fgDim)
            if repos.isEmpty {
                Text("no spend yet").font(Konjo.mono(11)).foregroundStyle(Konjo.fgMute)
                    .frame(maxWidth: .infinity).padding(.vertical, 8)
            } else {
                VStack(spacing: 8) {
                    ForEach(repos) { row in
                        HStack(spacing: 10) {
                            Text(row.name).font(Konjo.mono(11)).foregroundStyle(Konjo.fgDim)
                                .lineLimit(1).frame(width: 74, alignment: .leading)
                            GeometryReader { g in
                                ZStack(alignment: .leading) {
                                    Capsule().fill(Color.black.opacity(0.4))
                                    Capsule().fill(Konjo.stackTeal)
                                        .frame(width: g.size.width * CGFloat(row.cost / maxCost))
                                }
                            }
                            .frame(height: 6)
                            Text(String(format: "$%.2f", row.cost))
                                .font(Konjo.mono(11)).foregroundStyle(Konjo.stackTeal).monospacedDigit()
                                .frame(width: 52, alignment: .trailing)
                        }
                    }
                }
            }
        }
        .padding(16)
        .konjoSurface(12)
    }

    // MARK: By-model breakdown

    /// Cost grouped by model, billed today (UTC) — from `GET /api/budget/breakdown`.
    private var byModelBreakdown: some View {
        let maxCost = max(1, breakdown.byModel.map(\.costUsd).max() ?? 0)
        return VStack(alignment: .leading, spacing: 10) {
            Text("BY MODEL").font(Konjo.mono(9, weight: .semibold)).tracking(1).foregroundStyle(Konjo.fgDim)
            if breakdown.byModel.isEmpty {
                Text("no spend today").font(Konjo.mono(11)).foregroundStyle(Konjo.fgMute)
                    .frame(maxWidth: .infinity).padding(.vertical, 8)
            } else {
                VStack(spacing: 8) {
                    ForEach(breakdown.byModel) { row in
                        HStack(spacing: 10) {
                            Text(row.model).font(Konjo.mono(11)).foregroundStyle(Konjo.fgDim)
                                .lineLimit(1).frame(width: 74, alignment: .leading)
                            GeometryReader { g in
                                ZStack(alignment: .leading) {
                                    Capsule().fill(Color.black.opacity(0.4))
                                    Capsule().fill(Konjo.stackViolet)
                                        .frame(width: g.size.width * CGFloat(row.costUsd / maxCost))
                                }
                            }
                            .frame(height: 6)
                            Text(String(format: "$%.2f", row.costUsd))
                                .font(Konjo.mono(11)).foregroundStyle(Konjo.stackViolet).monospacedDigit()
                                .frame(width: 52, alignment: .trailing)
                        }
                    }
                }
            }
        }
        .padding(16)
        .konjoSurface(12)
    }

    // MARK: Top spenders

    private var topSpenders: some View {
        let spenders = model.liveAgents.values
            .filter { $0.costUsd > 0 }
            .sorted { $0.costUsd > $1.costUsd }
            .prefix(8)
        let maxCost = spenders.first?.costUsd ?? 1
        return VStack(alignment: .leading, spacing: 12) {
            Text("TOP SPENDERS").font(Konjo.mono(9, weight: .semibold)).tracking(1).foregroundStyle(Konjo.fgDim)
            if spenders.isEmpty {
                Text("no spend yet").font(Konjo.mono(11)).foregroundStyle(Konjo.fgMute)
                    .frame(maxWidth: .infinity).padding(.vertical, 10)
            } else {
                ForEach(Array(spenders), id: \.id) { a in
                    spenderRow(a, maxCost: maxCost)
                }
            }
        }
        .padding(16)
        .konjoSurface(12)
    }

    private func spenderRow(_ a: LiveAgent, maxCost: Double) -> some View {
        HStack(spacing: 12) {
            Circle().fill(a.active ? Konjo.jade : Konjo.fgMute).frame(width: 6, height: 6)
            VStack(alignment: .leading, spacing: 3) {
                Text(a.goal).font(Konjo.mono(11)).lineLimit(1).foregroundStyle(Konjo.fg)
                GeometryReader { g in
                    ZStack(alignment: .leading) {
                        Capsule().fill(Color.black.opacity(0.4))
                        Capsule().fill(Konjo.flame)
                            .frame(width: g.size.width * CGFloat(min(1, a.costUsd / maxCost)))
                    }
                }
                .frame(height: 3)
            }
            Text(String(format: "$%.4f", a.costUsd))
                .font(Konjo.mono(11)).foregroundStyle(Konjo.flame).monospacedDigit().frame(width: 64, alignment: .trailing)
            Button { Task { await model.cancelTask(a.id) } } label: {
                Image(systemName: "stop.fill").font(.system(size: 10)).foregroundStyle(Konjo.rose)
            }
            .buttonStyle(.plain).disabled(!a.active).opacity(a.active ? 1 : 0.25)
        }
    }

    /// Every entry in `model.budgetBreaches` (up to 5) — mirrors web's
    /// `budget/+page.svelte` "recent breaches" panel.
    private var breachHistory: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("RECENT BREACHES").font(Konjo.mono(9, weight: .semibold)).tracking(1).foregroundStyle(Konjo.rose)
            VStack(alignment: .leading, spacing: 6) {
                ForEach(Array(model.budgetBreaches.enumerated()), id: \.offset) { _, breach in
                    breachRow(breach)
                }
            }
        }
        .padding(14)
        .background(RoundedRectangle(cornerRadius: 12).fill(Konjo.rose.opacity(0.06)))
        .overlay(RoundedRectangle(cornerRadius: 12).stroke(Konjo.rose.opacity(0.3), lineWidth: 1))
    }

    private func breachRow(_ breach: BudgetBreach) -> some View {
        HStack(spacing: 8) {
            Text("◈").foregroundStyle(Konjo.rose)
            Text(breach.scope).font(Konjo.mono(11)).foregroundStyle(Konjo.fgDim)
            Text("·").foregroundStyle(Konjo.fgMute)
            Text(String(format: "$%.2f / $%.2f/h", breach.burnedUsd, breach.limitUsd))
                .font(Konjo.mono(11)).foregroundStyle(Konjo.fgDim).monospacedDigit()
        }
    }

    // MARK: Actions / helpers

    private func stopAll() {
        for a in model.liveAgents.values where a.active {
            Task { await model.cancelTask(a.id) }
        }
    }

    private func setCap(_ v: Double) {
        cap = v
        UserDefaults.standard.set(v, forKey: Self.capKey)
    }

    private func setAlertPct(_ v: Double) {
        alertPct = v
        UserDefaults.standard.set(v, forKey: Self.alertKey)
    }

    private func fmtMins(_ m: Double?) -> String {
        guard let m else { return "—" }
        if m < 1 { return "<1m" }
        if m < 60 { return "\(Int(m))m" }
        return String(format: "%.1fh", m / 60)
    }

    private static let capKey = "lopi.budget.cap"
    static func loadCap() -> Double {
        let v = UserDefaults.standard.double(forKey: capKey)
        return v > 0 ? v : 5
    }

    private static let alertKey = "lopi.budget.alertPct"
    /// Burn-fraction (% of cap) above which a budget alert should surface —
    /// mirrors web's `loadAlertPct` (10...100, default 80).
    static func loadAlertPct() -> Double {
        let v = UserDefaults.standard.double(forKey: alertKey)
        return v >= 10 && v <= 100 ? v : 80
    }
}

/// Burn-rate band relative to the cap (shared color logic with the web view).
enum BudgetState { case ok, warn, over }
