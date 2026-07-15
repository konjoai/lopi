import SwiftUI
import LopiStacksKit

/// Loop Engineering — the macOS cockpit for lopi's autonomous loops.
///
/// Mirrors the web Loop screen (Phase 16): the effective `.lopi/loop.toml` with
/// validation, the L1–L4 phased-autonomy ladder, each schedule's trust level
/// (the single writable control), discovered skills + rules, and the Konjo
/// quality gates that say "no" to the loop. Read-mostly by design.
struct LoopView: View {
    @Environment(AppModel.self) private var model

    /// The strategy whose self-prompt preview is shown; empty = the active one.
    @State private var focusedStrategy: String = ""

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                header
                if let snap = model.loopSnapshot {
                    if let health = model.loopHealth { healthPanel(health) }
                    runsPanel()
                    configPanel(snap.config)
                    ladderPanel(snap.autonomyLevels)
                    selfPromptPanel(snap)
                    schedulesPanel(snap)
                    HStack(alignment: .top, spacing: 16) {
                        skillsPanel(snap.skills)
                        rulesPanel(snap.rules)
                    }
                    gatesPanel(snap.gates)
                } else {
                    Text("loading loop config…")
                        .font(Konjo.mono(13)).foregroundStyle(Konjo.fgMute)
                        .padding(.top, 40)
                }
            }
            .padding(20)
            .frame(maxWidth: 920, alignment: .leading)
            .frame(maxWidth: .infinity)
        }
        .background(Konjo.bg)
        .task { await model.refreshLoop() }
        .refreshable { await model.refreshLoop() }
    }

    // MARK: Header

    private var header: some View {
        HStack(alignment: .bottom) {
            VStack(alignment: .leading, spacing: 4) {
                Text("Loop Engineering").font(Konjo.sans(24, weight: .bold)).foregroundStyle(Konjo.fg)
                Text("LOOP-AS-CODE · TRUST LEVELS · GUARDRAILS")
                    .font(Konjo.mono(10, weight: .semibold)).tracking(1.6)
                    .foregroundStyle(Konjo.fgMute)
            }
            Spacer()
            if let repo = model.loopSnapshot?.repo {
                VStack(alignment: .trailing, spacing: 2) {
                    Text("REPO").font(Konjo.mono(9)).tracking(1.4).foregroundStyle(Konjo.fgMute)
                    Text(repo).font(Konjo.mono(11)).foregroundStyle(Konjo.ice).lineLimit(1)
                }
            }
        }
    }

    // MARK: Loop Health (observability)

    private func healthPanel(_ h: LoopHealth) -> some View {
        KonjoPanel {
            VStack(alignment: .leading, spacing: 14) {
                panelHead("Loop Health", "observe · evaluate · improve") {
                    Text("\(h.stats.attempts) attempts · \(h.stats.runs) runs")
                        .font(Konjo.mono(10)).foregroundStyle(Konjo.fgMute)
                }
                let cols = Array(repeating: GridItem(.flexible(), alignment: .topLeading), count: 5)
                LazyVGrid(columns: cols, spacing: 10) {
                    statTile("Success", pct(h.stats.successRate), tint: rateColor(h.stats.successRate))
                    statTile("Verifier",
                             h.stats.verifierTotal == 0 ? "—" : pct(h.stats.verifierPassRate),
                             tint: Konjo.ice)
                    statTile("Runs", "\(h.stats.runs)")
                    statTile("Spend", String(format: "$%.2f", h.stats.spendUsd), tint: Konjo.sun)
                    statTile("Tokens", tokenLabel(h.stats.tokens))
                }
                if h.attempts.count >= 2 || h.burn.count >= 2 {
                    healthCharts(h)
                    outcomeBar(h.outcomes)
                } else {
                    Text("No loop telemetry yet — run a loop to populate metrics.")
                        .font(Konjo.mono(11)).foregroundStyle(Konjo.fgMute)
                }
            }
        }
    }

    private func healthCharts(_ h: LoopHealth) -> some View {
        let score = h.attempts.map(\.testPassRate)
        let pressure = h.burn.map(\.contextPressure)
        let diff = h.attempts.map { Double($0.diffLines) }
        let cost = h.burn.map(\.costUsd)
        let cols = [GridItem(.flexible(), spacing: 14), GridItem(.flexible(), spacing: 14)]
        return LazyVGrid(columns: cols, spacing: 14) {
            chartTile("Score / attempt", score.last.map { pct($0) } ?? "—", score, Konjo.jade)
            chartTile("Context pressure", pressure.last.map { pct($0) } ?? "—", pressure, Konjo.ice)
            chartTile("Diff size", diff.last.map { "\(Int($0))L" } ?? "—", diff, Konjo.ice)
            chartTile("Cost burn", String(format: "$%.2f", h.stats.spendUsd), cost, Konjo.sun)
        }
    }

    private func statTile(_ label: String, _ value: String, tint: Color = Konjo.fg) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(label.uppercased()).font(Konjo.mono(9)).tracking(1.2).foregroundStyle(Konjo.fgMute)
            Text(value).font(Konjo.sans(20, weight: .bold)).foregroundStyle(tint)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(10)
        .background(Konjo.bg2)
        .overlay(RoundedRectangle(cornerRadius: 8).stroke(Konjo.line, lineWidth: 1))
        .clipShape(RoundedRectangle(cornerRadius: 8))
    }

    private func chartTile(_ title: String, _ value: String,
                           _ samples: [Double], _ color: Color) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text(title.uppercased()).font(Konjo.mono(9)).tracking(1.2).foregroundStyle(Konjo.fgMute)
                Spacer()
                Text(value).font(Konjo.mono(10, weight: .medium)).foregroundStyle(color)
            }
            Sparkline(samples: samples, color: color).frame(height: 40)
        }
    }

    private func outcomeBar(_ outcomes: [LoopOutcome]) -> some View {
        let total = max(outcomes.reduce(0) { $0 + $1.count }, 1)
        return VStack(alignment: .leading, spacing: 8) {
            Text("OUTCOME DISTRIBUTION").font(Konjo.mono(9)).tracking(1.2).foregroundStyle(Konjo.fgMute)
            GeometryReader { geo in
                HStack(spacing: 0) {
                    ForEach(outcomes) { o in
                        Rectangle().fill(outcomeColor(o.label))
                            .frame(width: geo.size.width * CGFloat(o.count) / CGFloat(total))
                    }
                }
            }
            .frame(height: 10)
            .clipShape(Capsule())
            HStack(spacing: 14) {
                ForEach(outcomes) { o in
                    HStack(spacing: 5) {
                        Circle().fill(outcomeColor(o.label)).frame(width: 7, height: 7)
                        Text("\(o.label) · \(o.count)").font(Konjo.mono(10)).foregroundStyle(Konjo.fgDim)
                    }
                }
            }
        }
    }

    // MARK: Recent runs + per-run drill-down

    /// The four loop lifecycle stages shown per attempt for structure.
    private let stages = ["plan", "implement", "test", "score"]

    private func runsPanel() -> some View {
        KonjoPanel {
            VStack(alignment: .leading, spacing: 12) {
                panelHead("Recent Runs", "click a run for its attempt-by-attempt trace") { EmptyView() }
                if model.loopRuns.isEmpty {
                    Text("No runs yet — loop runs appear here once a task executes.")
                        .font(Konjo.mono(12)).foregroundStyle(Konjo.fgMute)
                } else {
                    ForEach(model.loopRuns) { runRow($0) }
                }
            }
        }
    }

    private func runRow(_ r: LoopRun) -> some View {
        let isOpen = model.selectedRun == r.taskId
        return VStack(alignment: .leading, spacing: 8) {
            Button { Task { await model.selectRun(r.taskId) } } label: {
                HStack(spacing: 10) {
                    Text(isOpen ? "▾" : "▸").font(Konjo.mono(10)).foregroundStyle(Konjo.fgMute)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(r.goal).font(Konjo.mono(12)).foregroundStyle(Konjo.fg).lineLimit(1)
                        Text("\(r.attempts) attempt\(r.attempts == 1 ? "" : "s") · best \(pct(r.bestScore))")
                            .font(Konjo.mono(10)).foregroundStyle(Konjo.fgMute)
                    }
                    Spacer(minLength: 8)
                    Text(r.finalOutcome.uppercased()).font(Konjo.mono(10)).tracking(1.2)
                        .foregroundStyle(outcomeColor(r.finalOutcome))
                }
                .padding(10)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(Konjo.bg2)
                .overlay(RoundedRectangle(cornerRadius: 8)
                    .stroke(isOpen ? Konjo.ice.opacity(0.5) : Konjo.line, lineWidth: 1))
                .clipShape(RoundedRectangle(cornerRadius: 8))
            }
            .buttonStyle(.plain)
            if isOpen { runTraceDetail() }
        }
    }

    @ViewBuilder
    private func runTraceDetail() -> some View {
        if model.traceLoading {
            Text("loading trace…").font(Konjo.mono(11)).foregroundStyle(Konjo.fgMute)
                .padding(.leading, 12)
        } else if let t = model.loopTrace {
            VStack(alignment: .leading, spacing: 8) {
                ForEach(t.attempts) { attemptCard($0) }
            }
            .padding(.leading, 12)
        }
    }

    private func attemptCard(_ a: LoopRunAttempt) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text("Attempt \(a.attempt)").font(Konjo.sans(13)).foregroundStyle(Konjo.fg)
                Spacer()
                Text(a.outcome.uppercased()).font(Konjo.mono(10)).tracking(1.2)
                    .foregroundStyle(outcomeColor(a.outcome))
            }
            HStack(spacing: 6) {
                ForEach(Array(stages.enumerated()), id: \.offset) { i, st in
                    Text(st.uppercased()).font(Konjo.mono(8)).tracking(1).foregroundStyle(Konjo.fgMute)
                    if i < stages.count - 1 {
                        Text("→").font(Konjo.mono(8)).foregroundStyle(Konjo.fgMute.opacity(0.5))
                    }
                }
            }
            HStack(spacing: 14) {
                metric("pass", pct(a.testPassRate), Konjo.jade)
                metric("lint", "\(a.lintErrors)", a.lintErrors > 0 ? Konjo.rose : Konjo.fgDim)
                metric("diff", "\(a.diffLines)L", Konjo.fgDim)
                metric("tok", tokenLabel(a.tokens), Konjo.fgDim)
                metric("cost", String(format: "$%.2f", a.costUsd), Konjo.sun)
            }
            if let v = a.verifier {
                Text("\(v.passed ? "✓ verifier passed" : "✗ verifier rejected") · \(pct(v.confidence))")
                    .font(Konjo.mono(10)).foregroundStyle(v.passed ? Konjo.jade : Konjo.rose)
                ForEach(v.gaps, id: \.self) { g in
                    Text("• \(g)").font(Konjo.mono(10)).foregroundStyle(Konjo.fgMute)
                }
            }
            ForEach(Array(a.errors.prefix(4)), id: \.self) { e in
                Text("• \(e)").font(Konjo.mono(10)).foregroundStyle(Konjo.rose.opacity(0.7)).lineLimit(1)
            }
        }
        .padding(10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Konjo.deep)
        .overlay(RoundedRectangle(cornerRadius: 8).stroke(Konjo.line, lineWidth: 1))
        .clipShape(RoundedRectangle(cornerRadius: 8))
    }

    private func metric(_ label: String, _ value: String, _ tint: Color) -> some View {
        HStack(spacing: 3) {
            Text(label).font(Konjo.mono(9)).foregroundStyle(Konjo.fgMute)
            Text(value).font(Konjo.mono(10, weight: .medium)).foregroundStyle(tint)
        }
    }

    // MARK: Config

    private func configPanel(_ c: LoopConfigDTO) -> some View {
        KonjoPanel {
            VStack(alignment: .leading, spacing: 14) {
                panelHead("Effective Config", ".lopi/loop.toml") {
                    Text(c.valid ? "VALID" : "\(c.issues.count) ISSUE(S)")
                        .font(Konjo.mono(10, weight: .semibold)).tracking(1.2)
                        .foregroundStyle(c.valid ? Konjo.jade : Konjo.rose)
                }
                let cols = [GridItem(.flexible(), alignment: .topLeading),
                            GridItem(.flexible(), alignment: .topLeading),
                            GridItem(.flexible(), alignment: .topLeading)]
                LazyVGrid(columns: cols, alignment: .leading, spacing: 14) {
                    field("Default Autonomy", "\(c.autonomyTag) · \(c.autonomyLabel)",
                          tint: levelColor(c.autonomyTag))
                    field("Vision Anchor", c.visionPath ?? "—")
                    field("Per-run Budget", c.budgetTokens == 0 ? "inherit global" : "\(c.budgetTokens) tokens")
                    field("No-progress Halt", "\(c.noProgressLimit) iterations")
                    field("Max Iterations", "\(c.maxIterations)")
                    field("Skills / Rules",
                          "\(c.skillsEnabled.isEmpty ? "all" : "\(c.skillsEnabled.count)") / \(c.rulesEnabled.isEmpty ? "all" : "\(c.rulesEnabled.count)")")
                }
                if !c.valid {
                    ForEach(c.issues, id: \.self) { issue in
                        Text("• \(issue)").font(Konjo.mono(11)).foregroundStyle(Konjo.rose)
                    }
                }
            }
        }
    }

    // MARK: Autonomy ladder

    private func ladderPanel(_ levels: [LoopAutonomyOption]) -> some View {
        KonjoPanel {
            VStack(alignment: .leading, spacing: 14) {
                panelHead("Autonomy Ladder", "L1 → L4 · trust earned incrementally") { EmptyView() }
                let cols = Array(repeating: GridItem(.flexible(), alignment: .topLeading), count: 4)
                LazyVGrid(columns: cols, spacing: 10) {
                    ForEach(levels) { l in
                        VStack(alignment: .leading, spacing: 3) {
                            Text(l.tag).font(Konjo.mono(14, weight: .bold)).foregroundStyle(levelColor(l.tag))
                            Text(l.label).font(Konjo.sans(13)).foregroundStyle(Konjo.fg)
                            Text(ladderHint(l)).font(Konjo.mono(10)).foregroundStyle(Konjo.fgMute)
                        }
                        .padding(10)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .background(Konjo.bg2)
                        .overlay(RoundedRectangle(cornerRadius: 8).stroke(Konjo.line, lineWidth: 1))
                        .clipShape(RoundedRectangle(cornerRadius: 8))
                    }
                }
            }
        }
    }

    // MARK: Self-prompting strategy (writable, loop-as-code)

    private func selfPromptPanel(_ snap: LoopSnapshot) -> some View {
        KonjoPanel {
            VStack(alignment: .leading, spacing: 12) {
                panelHead("Self-Prompting Strategy",
                          "how the loop re-prompts itself after a failed attempt") {
                    KonjoMenu(
                        title: "",
                        options: snap.selfPromptStrategies.map {
                            LaunchOption(value: $0.value, label: "\($0.tag) · \($0.label)")
                        },
                        value: Binding(
                            get: { snap.config.selfPrompt },
                            set: { newValue in
                                focusedStrategy = newValue
                                Task { await model.setLoopStrategy(newValue) }
                            }
                        ),
                        dense: true
                    )
                }
                Text("The single highest-leverage loop lever — the text the agent feeds back into "
                     + "its own next plan. Picking one writes .lopi/loop.toml; the runner honors it "
                     + "live on the next adaptive retry.")
                    .font(Konjo.mono(11)).foregroundStyle(Konjo.fgMute)
                let cols = Array(repeating: GridItem(.flexible(), alignment: .topLeading), count: 2)
                LazyVGrid(columns: cols, spacing: 10) {
                    ForEach(snap.selfPromptStrategies) { st in
                        strategyCard(st, active: st.value == snap.config.selfPrompt)
                    }
                }
                if let preview = previewStrategy(snap) {
                    VStack(alignment: .leading, spacing: 6) {
                        Text("SELF-PROMPT PREVIEW · \(preview.tag) · \(preview.label)")
                            .font(Konjo.mono(9)).tracking(1.2).foregroundStyle(Konjo.fgMute)
                        Text(preview.preview)
                            .font(Konjo.mono(11)).foregroundStyle(Konjo.fgDim)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(10)
                            .background(Konjo.bg2)
                            .overlay(RoundedRectangle(cornerRadius: 8).stroke(Konjo.line, lineWidth: 1))
                            .clipShape(RoundedRectangle(cornerRadius: 8))
                            .textSelection(.enabled)
                    }
                }
                escalationRow(snap.config)
            }
        }
    }

    /// Adaptive-escalation toggle + the per-attempt ladder it produces.
    private func escalationRow(_ c: LoopConfigDTO) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            Toggle(isOn: Binding(
                get: { c.escalateStrategy },
                set: { newValue in Task { await model.setLoopEscalation(newValue) } }
            )) {
                VStack(alignment: .leading, spacing: 2) {
                    Text("Adaptive escalation").font(Konjo.sans(13)).foregroundStyle(Konjo.fg)
                    Text("Climb one rung up the ladder each failed attempt.")
                        .font(Konjo.mono(10)).foregroundStyle(Konjo.fgMute)
                }
            }
            .toggleStyle(.switch)
            .tint(Konjo.jade)
            if c.escalateStrategy {
                HStack(spacing: 6) {
                    ForEach(Array(c.escalationLadder.enumerated()), id: \.element.id) { idx, rung in
                        if idx > 0 {
                            Text("→").font(Konjo.mono(10)).foregroundStyle(Konjo.fgMute)
                        }
                        HStack(spacing: 4) {
                            Text("#\(rung.attempt)").font(Konjo.mono(10)).foregroundStyle(Konjo.fgMute)
                            Text(rung.tag).font(Konjo.mono(10, weight: .bold))
                                .foregroundStyle(strategyColor(rung.tag))
                        }
                        .padding(.horizontal, 8).padding(.vertical, 4)
                        .background(Konjo.bg2)
                        .overlay(RoundedRectangle(cornerRadius: 6).stroke(Konjo.line2, lineWidth: 1))
                        .clipShape(RoundedRectangle(cornerRadius: 6))
                    }
                }
            }
        }
        .padding(10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Konjo.bg2)
        .overlay(RoundedRectangle(cornerRadius: 8).stroke(Konjo.line, lineWidth: 1))
        .clipShape(RoundedRectangle(cornerRadius: 8))
    }

    private func strategyCard(_ st: LoopSelfPromptOption, active: Bool) -> some View {
        Button {
            focusedStrategy = st.value
        } label: {
            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 8) {
                    Text(st.tag).font(Konjo.mono(13, weight: .bold)).foregroundStyle(strategyColor(st.tag))
                    Text(st.label).font(Konjo.sans(13)).foregroundStyle(Konjo.fg)
                    if active {
                        Spacer(minLength: 4)
                        Text("ACTIVE").font(Konjo.mono(8)).tracking(1.2).foregroundStyle(Konjo.jade)
                    }
                }
                Text(st.description).font(Konjo.mono(10)).foregroundStyle(Konjo.fgMute)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            .padding(10)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Konjo.bg2)
            .overlay(RoundedRectangle(cornerRadius: 8)
                .stroke(active ? Konjo.ice : Konjo.line, lineWidth: active ? 1.5 : 1))
            .clipShape(RoundedRectangle(cornerRadius: 8))
        }
        .buttonStyle(.plain)
    }

    /// The strategy to preview: the focused one, else the repo's active strategy.
    private func previewStrategy(_ snap: LoopSnapshot) -> LoopSelfPromptOption? {
        let target = focusedStrategy.isEmpty ? snap.config.selfPrompt : focusedStrategy
        return snap.selfPromptStrategies.first { $0.value == target }
            ?? snap.selfPromptStrategies.first
    }

    private func strategyColor(_ tag: String) -> Color {
        switch tag {
        case "S1": return Konjo.ice
        case "S2": return Konjo.jade
        case "S3": return Konjo.sun
        case "S4": return Konjo.ember
        default: return Konjo.ice
        }
    }

    // MARK: Schedules (writable trust level)

    private func schedulesPanel(_ snap: LoopSnapshot) -> some View {
        KonjoPanel {
            VStack(alignment: .leading, spacing: 12) {
                panelHead("Scheduled Loops", "set each loop's trust level") { EmptyView() }
                if snap.schedules.isEmpty {
                    Text("No scheduled loops — add one from the Cron screen.")
                        .font(Konjo.mono(12)).foregroundStyle(Konjo.fgMute)
                } else {
                    ForEach(snap.schedules) { s in scheduleRow(s, snap.autonomyLevels) }
                }
            }
        }
    }

    private func scheduleRow(_ s: LoopSchedule, _ levels: [LoopAutonomyOption]) -> some View {
        HStack(spacing: 12) {
            Circle().fill(s.enabled ? Konjo.jade : Konjo.fgMute)
                .frame(width: 8, height: 8)
            VStack(alignment: .leading, spacing: 2) {
                Text(s.name).font(Konjo.mono(12)).foregroundStyle(Konjo.fg).lineLimit(1)
                Text("\(s.cron) · \(s.goal)").font(Konjo.mono(10)).foregroundStyle(Konjo.fgMute).lineLimit(1)
            }
            Spacer(minLength: 8)
            KonjoMenu(
                title: "",
                options: levels.map { LaunchOption(value: $0.value, label: "\($0.tag) · \($0.label)") },
                value: Binding(
                    get: { s.autonomyLevel },
                    set: { newValue in Task { await model.setScheduleAutonomy(s.id, level: newValue) } }
                ),
                dense: true
            )
        }
        .padding(10)
        .background(Konjo.bg2)
        .overlay(RoundedRectangle(cornerRadius: 8).stroke(Konjo.line, lineWidth: 1))
        .clipShape(RoundedRectangle(cornerRadius: 8))
    }

    // MARK: Skills + rules

    private func skillsPanel(_ skills: [LoopSkill]) -> some View {
        KonjoPanel {
            VStack(alignment: .leading, spacing: 10) {
                panelHead("Skills", "\(skills.count) discovered") { EmptyView() }
                if skills.isEmpty {
                    Text("no skills").font(Konjo.mono(11)).foregroundStyle(Konjo.fgMute)
                } else {
                    ForEach(skills) { sk in
                        VStack(alignment: .leading, spacing: 2) {
                            Text(sk.name).font(Konjo.mono(12)).foregroundStyle(Konjo.ice)
                            if !sk.description.isEmpty {
                                Text(sk.description).font(Konjo.mono(10)).foregroundStyle(Konjo.fgMute)
                            }
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .topLeading)
    }

    private func rulesPanel(_ rules: [LoopRule]) -> some View {
        KonjoPanel {
            VStack(alignment: .leading, spacing: 10) {
                panelHead("Rules", "\(rules.count) active") { EmptyView() }
                if rules.isEmpty {
                    Text("no rules").font(Konjo.mono(11)).foregroundStyle(Konjo.fgMute)
                } else {
                    FlowChips(rules.map(\.name))
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .topLeading)
    }

    // MARK: Gates

    private func gatesPanel(_ gates: [LoopGate]) -> some View {
        KonjoPanel {
            VStack(alignment: .leading, spacing: 12) {
                panelHead("Quality Gates", "Konjo three-wall framework — the loop's 'no'") { EmptyView() }
                ForEach(gates) { g in
                    VStack(alignment: .leading, spacing: 3) {
                        HStack(spacing: 8) {
                            Text(g.wall).font(Konjo.mono(11, weight: .bold)).foregroundStyle(Konjo.sun)
                            Text(g.name).font(Konjo.sans(13)).foregroundStyle(Konjo.fg)
                        }
                        Text(g.checks).font(Konjo.mono(10)).foregroundStyle(Konjo.fgMute)
                    }
                    .padding(10)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(Konjo.bg2)
                    .overlay(RoundedRectangle(cornerRadius: 8).stroke(Konjo.line, lineWidth: 1))
                    .clipShape(RoundedRectangle(cornerRadius: 8))
                }
            }
        }
    }

    // MARK: Helpers

    private func panelHead<Trailing: View>(
        _ title: String, _ subtitle: String, @ViewBuilder trailing: () -> Trailing
    ) -> some View {
        HStack(alignment: .firstTextBaseline) {
            VStack(alignment: .leading, spacing: 2) {
                Text(title).font(Konjo.sans(15, weight: .bold)).foregroundStyle(Konjo.fg)
                Text(subtitle.uppercased()).font(Konjo.mono(9)).tracking(1.4).foregroundStyle(Konjo.fgMute)
            }
            Spacer()
            trailing()
        }
    }

    private func field(_ label: String, _ value: String, tint: Color = Konjo.fgDim) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(label.uppercased()).font(Konjo.mono(9)).tracking(1.2).foregroundStyle(Konjo.fgMute)
            Text(value).font(Konjo.mono(12)).foregroundStyle(tint)
        }
    }

    private func ladderHint(_ l: LoopAutonomyOption) -> String {
        if l.allowsAutoMerge { return "auto-merge on pass" }
        if l.requiresVerifier { return "verify before PR" }
        if l.opensPr { return "draft PR, human approves" }
        return "report only, no PR"
    }

    private func levelColor(_ tag: String) -> Color {
        switch tag {
        case "L1": return Konjo.ice
        case "L2": return Konjo.jade
        case "L3": return Konjo.sun
        case "L4": return Konjo.ember
        default: return Konjo.ice
        }
    }

    /// Format a 0…1 ratio as a whole-number percentage.
    private func pct(_ x: Double) -> String { "\(Int((x * 100).rounded()))%" }

    /// Compact token count: `1.2k` past a thousand, else the raw integer.
    private func tokenLabel(_ t: Int) -> String {
        t >= 1000 ? String(format: "%.1fk", Double(t) / 1000) : "\(t)"
    }

    /// Heat a success rate: calm jade when healthy, warming toward rose as it drops.
    private func rateColor(_ x: Double) -> Color {
        x >= 0.8 ? Konjo.jade : (x >= 0.5 ? Konjo.sun : Konjo.rose)
    }

    /// Outcome → accent. success is calm jade; stuck/failed runs heat up.
    private func outcomeColor(_ label: String) -> Color {
        switch label {
        case "success": return Konjo.jade
        case "retry": return Konjo.sun
        default: return Konjo.rose
        }
    }
}

/// A simple wrapping chip row for short labels (rules).
private struct FlowChips: View {
    let items: [String]
    init(_ items: [String]) { self.items = items }

    var body: some View {
        // A lightweight wrap using a fixed 2-column grid keeps layout stable
        // without a custom Layout; rule lists are short.
        let cols = [GridItem(.adaptive(minimum: 110), spacing: 8, alignment: .leading)]
        LazyVGrid(columns: cols, alignment: .leading, spacing: 8) {
            ForEach(items, id: \.self) { name in
                Text(name)
                    .font(Konjo.mono(11)).foregroundStyle(Konjo.fgDim)
                    .padding(.horizontal, 10).padding(.vertical, 5)
                    .background(Konjo.bg2)
                    .overlay(RoundedRectangle(cornerRadius: 6).stroke(Konjo.line2, lineWidth: 1))
                    .clipShape(RoundedRectangle(cornerRadius: 6))
            }
        }
    }
}
