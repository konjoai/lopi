import SwiftUI

/// StackControlDockView — the purple stack control area at the base of each pane
/// (Stack-1). Reuses the exact per-loop controls (the iteration-pill stepper and
/// the schedule/guards/evals/config popovers), scoped to the whole stack, plus
/// the pane's real run/pause/resume/drain machinery. A collapsible strip: header
/// (STACK chip + summary + chevron) always visible, controls expand in the
/// middle, run pinned at the bottom.
struct StackControlDockView: View {
    var store: StackStore
    var engine: StackRunEngine
    var pane: StackPaneState
    var index: Int
    var repoOptions: [StackOption]

    @State private var dockOpen = false
    @State private var schedOpen = false
    @State private var guardOpen = false
    @State private var evalOpen = false
    @State private var cfgOpen = false
    @State private var runMenuOpen = false
    @State private var dryRunResult: DryRunResult?

    private var config: StackConfig { pane.config }
    private var defaults: PaneDefaults { PaneDefaults(config.defaults) }
    private var scheduledOn: Bool { config.scheduled }
    private var guardsOn: Bool { stackGuardActive(config.guardrails) }
    private var evalsOn: Bool { stackEvalActive(config) }
    private var configOn: Bool { stackDefaultsActive(config.defaults) }
    private var goalOn: Bool { stackGoalActive(config) }
    private var pursues: Bool { stackPursuesGoal(config) }
    private var showSummary: Bool { scheduledOn || guardsOn || evalsOn || configOn || goalOn }
    private var modelLabel: String { MODEL_OPTIONS.first { $0.value == config.defaults.model }?.label ?? config.defaults.model }
    private var dockSummary: String {
        (scheduledOn ? cronHuman(config.cron) + " · " : "") + "loop ×\(maxIterationsLabel(config.loopCount)) · \(modelLabel)"
    }

    private var runState: StackRunState? { engine.run(for: pane.key) }
    private var phase: RunPhase? { runState?.phase }
    private var stopReason: StackStopReason? { runState?.stopReason }
    private var runError: String? { runState?.error }

    var body: some View {
        VStack(spacing: 0) {
            header
            if dockOpen { dockBody.transition(.opacity) }
            runArea
        }
        .padding(.horizontal, 16).padding(.vertical, 14)
        .background(LinearGradient(colors: [Konjo.stackViolet.opacity(0.22), Konjo.stackViolet.opacity(0.12)], startPoint: .top, endPoint: .bottom))
        .overlay(Rectangle().fill(Konjo.stackViolet.opacity(0.55)).frame(height: 1.5), alignment: .top)
    }

    // MARK: Header

    private var header: some View {
        HStack(spacing: 11) {
            stackChip
            if !dockOpen {
                Text(dockSummary).font(Konjo.mono(10)).foregroundStyle(Konjo.fgDim).lineLimit(1)
            }
            Spacer(minLength: 0)
            Button { withAnimation(.easeInOut(duration: 0.2)) { dockOpen.toggle() } } label: {
                Image(systemName: "chevron.up").font(.system(size: 13, weight: .bold)).foregroundStyle(Konjo.stackViolet)
                    .rotationEffect(.degrees(dockOpen ? 180 : 0))
                    .frame(width: 34, height: 34)
                    .overlay(RoundedRectangle(cornerRadius: 7).stroke(Konjo.line2, lineWidth: 1))
            }
            .buttonStyle(.plain).help("stack controls")
        }
        .padding(.bottom, dockOpen ? 6 : 0)
    }

    private var stackChip: some View {
        Text("STACK").font(Konjo.mono(9, weight: .bold)).tracking(1.4).foregroundStyle(Konjo.deep)
            .padding(.horizontal, 10).padding(.vertical, 3)
            .background(Konjo.stackViolet).clipShape(RoundedRectangle(cornerRadius: 4))
    }

    // MARK: Dock body — summary lines + cardbar

    private var dockBody: some View {
        VStack(alignment: .leading, spacing: 8) {
            if showSummary {
                if scheduledOn {
                    SummaryRow(systemImage: "clock", label: "schedule", accent: FacetAccent.schedule, text: cronHuman(config.cron))
                    Text("not yet enforced — no whole-chain cron exists server-side yet").font(Konjo.mono(9)).foregroundStyle(Konjo.fgMute).padding(.leading, 66)
                }
                if guardsOn { SummaryRow(systemImage: "shield", label: "guards", accent: FacetAccent.guards, text: stackGuardSummary(config.guardrails)) }
                if evalsOn { SummaryRow(systemImage: "checkmark.square", label: "evals", accent: FacetAccent.evals, text: stackEvalsSummary(config)) }
                if goalOn {
                    SummaryRow(systemImage: "gauge.with.dots.needle.67percent", label: "goal", accent: FacetAccent.goal, text: stackGoalSummary(config))
                    if !pursues {
                        Text("add chain-acceptance evals for the goal to pursue — a goal with nothing to check is inert")
                            .font(Konjo.mono(9)).foregroundStyle(Konjo.fgMute).padding(.leading, 66)
                    }
                }
                if configOn { SummaryRow(systemImage: "slider.horizontal.3", label: "default", accent: Konjo.stackViolet, text: stackDefaultsSummary(config.defaults)) }
            }
            cardbar
        }
        .padding(.top, 6)
    }

    private var cardbar: some View {
        HStack(spacing: 6) {
            IterationPill(value: config.loopCount) { delta in
                store.updateStackConfig(pane.key) { $0.loopCount = stepMaxIterations($0.loopCount, delta) }
            }
            CardbarButton(systemImage: "clock", active: scheduledOn, accent: FacetAccent.schedule, help: "schedule the stack") { schedOpen = true }
                .popover(isPresented: $schedOpen, arrowEdge: .top) { schedulePopover }
            CardbarButton(systemImage: "shield", active: guardsOn, accent: FacetAccent.guards, help: "stack guardrails") { guardOpen = true }
                .popover(isPresented: $guardOpen, arrowEdge: .top) { guardsPopover }
            CardbarButton(systemImage: "checkmark.square", active: evalsOn, accent: FacetAccent.evals, count: config.evals.count, help: "stack evals") { evalOpen = true }
                .popover(isPresented: $evalOpen, arrowEdge: .top) { evalsPopover }
            CardbarButton(systemImage: "gauge.with.dots.needle.67percent", active: goalOn, accent: FacetAccent.goal, help: "run until the stack acceptance passes") {
                store.updateStackConfig(pane.key) { $0.goal.pursue.toggle() }
            }
            CardbarButton(systemImage: "slider.horizontal.3", active: configOn, accent: FacetAccent.config, help: "stack default config") { cfgOpen = true }
                .popover(isPresented: $cfgOpen, arrowEdge: .top) { configPopover }
            Spacer()
            CardbarButton(systemImage: "plus.square.on.square", help: "duplicate stack") { store.duplicateStackInPanes(pane.key) }
            CardbarButton(systemImage: "line.3.horizontal", help: "drag to reorder stacks") {}
            CardbarButton(systemImage: "trash", accent: Konjo.rose, danger: true, help: "delete stack") {
                engine.clearRun(pane.key); store.deleteStackFromPanes(pane.key)
            }
        }
    }

    // MARK: Run area

    private var runArea: some View {
        VStack(spacing: 9) {
            if let stopReason {
                banner(stackStopLabel(stopReason), ok: stopReason == .goalMet) { engine.clearRun(pane.key) }
            } else if let runError {
                banner(runError, ok: false) { engine.clearRun(pane.key) }
            } else if let dryRunResult {
                banner(dryRunText(dryRunResult), ok: dryRunResult.valid) { self.dryRunResult = nil }
            }
            runSplit
        }
        .padding(.top, 13)
    }

    private var runSplit: some View {
        HStack(spacing: 0) {
            Button(action: runMain) {
                HStack(spacing: 9) {
                    Image(systemName: phase == .running ? "pause.fill" : "play.fill").font(.system(size: 13, weight: .bold))
                    Text(runLabel).font(Konjo.sans(13, weight: .bold))
                }
                .frame(maxWidth: .infinity).padding(.vertical, 12)
                .background(LinearGradient(colors: [Color(hex: 0xFFB648), Konjo.flame], startPoint: .top, endPoint: .bottom))
                .foregroundStyle(Color(hex: 0x231000))
            }
            .buttonStyle(.plain).disabled(phase == .draining)
            Button { runMenuOpen.toggle() } label: {
                Image(systemName: "chevron.up").font(.system(size: 12, weight: .bold)).foregroundStyle(Color(hex: 0x231000))
                    .padding(.horizontal, 13).padding(.vertical, 14)
                    .background(Color(hex: 0xF08600))
            }
            .buttonStyle(.plain)
            .popover(isPresented: $runMenuOpen, arrowEdge: .top) {
                RunMenuView(store: store, engine: engine, paneKey: pane.key, defaults: defaults, phase: phase,
                            onDryRun: { dryRunResult = $0 }, onClose: { runMenuOpen = false })
            }
        }
        .clipShape(RoundedRectangle(cornerRadius: 8))
        .shadow(color: Konjo.flame.opacity(0.3), radius: 8, y: 4)
    }

    private var runLabel: String {
        switch phase {
        case .running: return "pause"
        case .paused: return "resume"
        case .draining: return "draining…"
        default: return pursues ? "pursue goal" : "run stack"
        }
    }

    private func runMain() {
        switch phase {
        case .running: engine.pauseStack(pane.key)
        case .paused: engine.resumeStack(pane.key, defaults)
        default: dryRunResult = nil; engine.runStack(pane.key, .run, defaults)
        }
        runMenuOpen = false
    }

    private func dryRunText(_ r: DryRunResult) -> String {
        if r.valid { return "dry run: \(r.plan.count) loop\(r.plan.count == 1 ? "" : "s") would run, in order" }
        return "dry run found \(r.issues.count) issue\(r.issues.count == 1 ? "" : "s"): \(r.issues.first?.message ?? "")"
    }

    private func banner(_ text: String, ok: Bool, dismiss: @escaping () -> Void) -> some View {
        HStack(spacing: 10) {
            Text(text).font(Konjo.mono(11)).foregroundStyle(ok ? Konjo.jade : (text.hasPrefix("dry run:") ? Konjo.fgDim : Konjo.rose)).lineLimit(2)
            Spacer(minLength: 0)
            Button(action: dismiss) { Image(systemName: "xmark").font(.system(size: 11)).foregroundStyle(Konjo.fgDim) }.buttonStyle(.plain)
        }
        .padding(.horizontal, 12).padding(.vertical, 8)
        .background((ok ? Konjo.jade : Konjo.rose).opacity(0.1))
        .overlay(RoundedRectangle(cornerRadius: 8).stroke((ok ? Konjo.jade : Konjo.rose).opacity(0.4), lineWidth: 1))
        .clipShape(RoundedRectangle(cornerRadius: 8))
    }

    // MARK: Stack-scoped popovers

    private var schedulePopover: some View {
        SchedulePopoverView(scheduled: config.scheduled, cron: config.cron,
            onToggle: { store.updateStackConfig(pane.key) { $0.scheduled.toggle() } },
            onChange: { next in store.updateStackConfig(pane.key) { $0.cron = next } })
    }
    private var guardsPopover: some View {
        GuardrailsPopoverView(scope: .stack,
            guardrails: Guardrails(gate: false, gateCmd: "", until: false, untilCmd: "", onFail: config.guardrails.onFail, budget: config.guardrails.budget),
            maxIterations: config.loopCount, iterLabel: "loop stack",
            onChange: { g in store.updateStackConfig(pane.key) { $0.guardrails = StackGuardrails(onFail: g.onFail, budget: g.budget) } },
            onStep: { delta in store.updateStackConfig(pane.key) { $0.loopCount = stepMaxIterations($0.loopCount, delta) } })
    }
    private var evalsPopover: some View {
        EvalsPopoverView(evals: config.evals, heading: "chain acceptance") { evals in store.updateStackConfig(pane.key) { $0.evals = evals } }
    }
    private var configPopover: some View {
        StackConfigPopoverView(defaults: config.defaults, repoOptions: repoOptions) { next in store.updateStackConfig(pane.key) { $0.defaults = next } }
    }
}
