import SwiftUI
import LopiStacksKit

/// StackControlDockView — the purple stack control area at the base of each pane
/// (Stack-1). Reuses the exact per-loop controls (the iteration-pill stepper and
/// the schedule/guards/evals/config popovers), scoped to the whole stack, plus
/// the pane's real run/pause/resume/drain machinery. A collapsible strip: header
/// (STACK chip + summary + chevron) always visible, controls expand in the
/// middle, run pinned at the bottom.
struct StackControlDockView: View {
    @Environment(AppModel.self) private var model
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
    @State private var goalOpen = false
    @State private var runMenuOpen = false
    @State private var runMainHeight: CGFloat = 0
    @State private var dryRunResult: DryRunResult?

    // ── stack command bar (`@repo` / `/command`) ────────────────────────────
    // The stack-only analogue of a card's goal-field autocomplete (Stack-1
    // §4): loop count, stack schedule/guardrails/run-until-goal have no
    // card-level equivalent to piggyback on, so they need their own
    // text-entry surface. Same `@`/`/` grammar as `StackCardView`'s composer,
    // writing to `pane.config` instead of a card's `config`.
    @State private var cmdText = ""
    @FocusState private var cmdBarFocused: Bool
    @State private var cmdActiveIndex = 0
    @State private var cmdDismissed = false
    @State private var pendingCommand: String?
    @State private var cmdBarHeight: CGFloat = 34

    private enum CmdMatch {
        case command(CommandSuggestion)
        case value(CommandValueSuggestion)
        var token: String { switch self { case .command(let c): return c.token; case .value(let v): return v.token } }
        var label: String { switch self { case .command(let c): return c.label; case .value(let v): return v.label } }
        var hint: String { switch self { case .command(let c): return c.hint; case .value(let v): return v.hint } }
    }

    private var repoMatches: [RepoSuggestion] { repoAutocomplete(cmdText, repoOptions) }
    private var showRepoBarSuggest: Bool { cmdBarFocused && !cmdDismissed && pendingCommand == nil && !repoMatches.isEmpty }

    private func commandOptionsFor(_ command: String) -> [StackOption] {
        switch command {
        case "model": return modelOptionsFrom(model.modelCatalog)
        case "effort": return effortOptionsFor(model.modelCatalog, model: config.defaults.model)
        case "autonomy": return AUTONOMY_OPTIONS
        case "branch": return (model.branchesByRepo[config.defaults.repo] ?? []).map { StackOption(value: $0, label: $0) }
        case "eval": return evalSuiteOptions()
        case "loop": return [
            StackOption(value: "1", label: "1 (off)"),
            StackOption(value: "2", label: "2"),
            StackOption(value: "3", label: "3"),
            StackOption(value: "5", label: "5"),
            StackOption(value: "10", label: "10"),
            StackOption(value: "0", label: "∞ (unlimited)")
        ]
        default: return []
        }
    }

    private var cmdMatches: [CmdMatch] {
        if let pendingCommand {
            return commandValueAutocomplete(cmdText, pendingCommand, commandOptionsFor(pendingCommand)).map { .value($0) }
        }
        return commandAutocomplete(cmdText, STACK_COMMANDS).map { .command($0) }
    }
    private var showCmdBarSuggest: Bool { cmdBarFocused && !cmdDismissed && !showRepoBarSuggest && !cmdMatches.isEmpty }

    private func applyCommandValue(_ command: String, _ value: String) {
        switch command {
        case "eval": store.updateStackConfig(pane.key) { $0.evals = applySuite($0.evals, EVAL_SUITES[value] ?? []) }
        case "loop": store.updateStackConfig(pane.key) { $0.loopCount = Int(value) ?? 0 }
        case "model": store.updateStackConfig(pane.key) { $0.defaults.model = value }
        case "effort": store.updateStackConfig(pane.key) { $0.defaults.effort = value }
        case "branch": store.updateStackConfig(pane.key) { $0.defaults.branch = value }
        case "autonomy": store.updateStackConfig(pane.key) { $0.defaults.autonomy = value }
        default: break
        }
    }

    private func fireCommandAction(_ command: String) {
        if command == "guard" { guardOpen = true }
        else if command == "schedule" { schedOpen = true }
        else if command == "goal" { goalOpen = true }
    }

    private func selectRepoFromBar(_ token: String) {
        if let suggestion = repoMatches.first(where: { $0.token == token }) {
            store.updateStackConfig(pane.key) { $0.defaults.repo = suggestion.value }
        }
        cmdText = ""
        cmdActiveIndex = 0
    }

    private func selectCommandFromBar(_ token: String) {
        if let pending = pendingCommand {
            if case .value(let suggestion)? = cmdMatches.first(where: { $0.token == token }) {
                applyCommandValue(pending, suggestion.value)
            }
            pendingCommand = nil
            cmdText = ""
        } else {
            let command = String(token.dropFirst())
            let def = STACK_COMMANDS.first(where: { $0.command == command })
            if def?.isValuePicker == true {
                cmdText = "/\(command)/"
                pendingCommand = command
            } else {
                fireCommandAction(command)
                cmdText = ""
            }
        }
        cmdActiveIndex = 0
    }

    private var config: StackConfig { pane.config }
    private var defaults: PaneDefaults { PaneDefaults(config.defaults) }
    private var scheduledOn: Bool { config.scheduled }
    private var guardsOn: Bool { stackGuardActive(config.guardrails) }
    private var evalsOn: Bool { stackEvalActive(config) }
    private var configOn: Bool { stackDefaultsActive(config.defaults) }
    private var goalOn: Bool { stackGoalActive(config) }
    private var pursues: Bool { stackPursuesGoal(config) }
    private var showSummary: Bool { scheduledOn || guardsOn || evalsOn || configOn || goalOn }
    private var modelLabel: String {
        modelOptionsFrom(model.modelCatalog).first { $0.value == config.defaults.model }?.label ?? config.defaults.model
    }
    /// A chosen repo previously vanished from the dock's own summary the
    /// instant it was set — visible in the config popover (`StackConfigPopoverView`
    /// reads `defaults.repo` directly) but nowhere else. Mirrors web's `repoLabel`.
    private var repoLabel: String? { config.defaults.repo.isEmpty ? nil : repoLabelForPath(config.defaults.repo, repoOptions) }
    private var loopLabel: String {
        let label = maxIterationsLabel(config.loopCount)
        return config.loopCount <= 1 ? label : "×\(label)"
    }
    private var dockSummary: String {
        (scheduledOn ? cronHuman(config.cron) + " · " : "") + "loop \(loopLabel) · \(modelLabel)"
    }

    private var runState: StackRunState? { engine.run(for: pane.key) }
    private var phase: RunPhase? { runState?.phase }
    private var stopReason: StackStopReason? { runState?.stopReason }
    private var runError: String? { runState?.error }
    /// The stack's own loop reads as "actively running" once it's mid-run
    /// with a real chain repeat configured (`loopCount != 1` — 1 is this
    /// pill's own "off" sentinel). `repetition` is 0-indexed and counts
    /// *completed* passes, so the live iteration is `repetition + 1`. Mirrors
    /// web's `stackLoopRunning`/`stackIterLabel`.
    private var loopRunningLabel: String? {
        guard phase == .running, config.loopCount != 1, let runState else { return nil }
        let total = runState.loopTarget
        return total == 0 ? "\(runState.repetition + 1)/∞" : "\(runState.repetition + 1)/\(total)"
    }

    var body: some View {
        VStack(spacing: 0) {
            header
            if dockOpen { dockBody.transition(.opacity) }
            runArea
        }
        .padding(.horizontal, 16).padding(.vertical, 14)
        // Rounded at the bottom two corners only (this is the pane's last child,
        // sitting flush against its bottom edge) via a shape fill, not a clip —
        // `StackPaneView` no longer clips its own frame (see its doc comment), so
        // this dock is what has to supply its own corner rounding without
        // clipping the command-bar autocomplete dropdown that overflows past it.
        .background(UnevenRoundedRectangle(bottomLeadingRadius: 14, bottomTrailingRadius: 14)
            .fill(LinearGradient(colors: [Konjo.stackViolet.opacity(0.22), Konjo.stackViolet.opacity(0.12)], startPoint: .top, endPoint: .bottom)))
        .overlay(Rectangle().fill(Konjo.stackViolet.opacity(0.55)).frame(height: 1.5), alignment: .top)
        .task { await model.ensureModelCatalog() }
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
                    .contentShape(Rectangle())
                    .accessibilityIdentifier("stack.dockExpand")
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
            commandBar
            if showSummary {
                if scheduledOn {
                    SummaryRow(systemImage: "clock", label: "schedule", accent: FacetAccent.schedule, text: cronHuman(config.cron))
                }
                if guardsOn { SummaryRow(systemImage: "shield", label: "guards", accent: FacetAccent.guards, text: stackGuardSummary(config.guardrails)) }
                if evalsOn { SummaryRow(systemImage: "checkmark.square", label: "evals", accent: FacetAccent.evals, text: stackEvalsSummary(config)) }
                if goalOn {
                    SummaryRow(systemImage: "gauge", label: "goal", accent: FacetAccent.goal, text: stackGoalSummary(config))
                    if !pursues {
                        Text("add chain-acceptance evals for the goal to pursue — a goal with nothing to check is inert")
                            .font(Konjo.mono(9)).foregroundStyle(Konjo.fgMute).padding(.leading, 66)
                    }
                }
                if configOn { SummaryRow(systemImage: "slider.horizontal.3", label: "default", accent: Konjo.stackViolet, text: stackDefaultsSummary(config.defaults, repoLabel: repoLabel)) }
            }
            cardbar
        }
        .padding(.top, 6)
    }

    private var cardbar: some View {
        HStack(spacing: 6) {
            IterationPill(value: config.loopCount, runningLabel: loopRunningLabel) { delta in
                store.updateStackConfig(pane.key) { $0.loopCount = stepMaxIterations($0.loopCount, delta) }
            }
            CardbarButton(systemImage: "clock", active: scheduledOn, accent: FacetAccent.schedule, help: "Schedule the entire stack") { schedOpen = true }
                .popover(isPresented: $schedOpen, arrowEdge: .top) { schedulePopover }
            CardbarButton(systemImage: "shield", active: guardsOn, accent: FacetAccent.guards, help: "stack guardrails") { guardOpen = true }
                .popover(isPresented: $guardOpen, arrowEdge: .top) { guardsPopover }
            CardbarButton(systemImage: "checkmark.square", active: evalsOn, accent: FacetAccent.evals, count: config.evals.count, help: "stack evals") { evalOpen = true }
                .popover(isPresented: $evalOpen, arrowEdge: .top) { evalsPopover }
            CardbarButton(systemImage: "gauge", active: goalOn, accent: FacetAccent.goal, help: "run until the stack acceptance passes") { goalOpen = true }
                .popover(isPresented: $goalOpen, arrowEdge: .top) { goalPopover }
            CardbarButton(systemImage: "slider.horizontal.3", active: configOn, accent: FacetAccent.config, help: "stack default config") { cfgOpen = true }
                .popover(isPresented: $cfgOpen, arrowEdge: .top) { configPopover }
            Spacer()
            StackTemplatesMenuView(store: store, templateStore: model.stackTemplateStore, paneKey: pane.key, cards: pane.cards)
            CardbarButton(systemImage: "square.on.square", help: "duplicate stack") { store.duplicateStackInPanes(pane.key) }
            dragHandle
            CardbarButton(systemImage: "trash", accent: Konjo.rose, danger: true, help: "delete stack") {
                engine.clearRun(pane.key); store.deleteStackFromPanes(pane.key)
            }
        }
    }

    /// Same visual chrome as `CardbarButton`, deliberately NOT a `Button`
    /// (a `.draggable()`/gesture chained onto an actual `Button` loses to
    /// the button's own tap recognizer). Doesn't carry `.draggable()`
    /// itself — pressing it sets `model.armedStackDragIndex`, which
    /// `ForgeView`'s grid uses to attach `.draggable()` to the WHOLE pane
    /// for exactly the press's duration, mirroring web's `armDrag`/
    /// `disarmDrag`. Putting `.draggable()` on the handle alone only made
    /// the small icon draggable, not the stack pane.
    private var dragHandle: some View {
        Image(systemName: "line.3.horizontal").font(.system(size: 12))
            .padding(.horizontal, 7)
            .frame(minWidth: 29, minHeight: 29)
            .foregroundStyle(Konjo.fgMute)
            .overlay(RoundedRectangle(cornerRadius: 6).stroke(Konjo.line, lineWidth: 1))
            .contentShape(Rectangle())
            .help("drag to reorder stacks")
            .accessibilityIdentifier("drag to reorder stacks")
            .gesture(
                DragGesture(minimumDistance: 0, coordinateSpace: .local)
                    .onChanged { _ in model.armedStackDragIndex = index }
                    .onEnded { _ in model.armedStackDragIndex = nil }
            )
    }

    // MARK: Command bar

    private var commandBar: some View {
        TextField("@org/repo /model /effort /branch /autonomy /loop /guard /schedule /eval /goal", text: $cmdText)
            .textFieldStyle(.plain).font(Konjo.mono(12)).foregroundStyle(Konjo.fg)
            .focused($cmdBarFocused)
            .onChange(of: cmdText) { _, newText in
                cmdDismissed = false
                // Re-infer `pendingCommand` from the typed text on every
                // change — see `StackCardView`'s identical comment for why
                // relying only on `selectCommandFromBar`'s explicit
                // assignment misses hand-typed `/model/`.
                if let inferred = detectPendingCommand(newText, STACK_COMMANDS) {
                    pendingCommand = inferred
                } else if let pending = pendingCommand, !newText.contains("/\(pending)/") {
                    pendingCommand = nil
                }
            }
            .onKeyPress(.downArrow) {
                if showRepoBarSuggest { cmdActiveIndex = (cmdActiveIndex + 1) % repoMatches.count; return .handled }
                if showCmdBarSuggest { cmdActiveIndex = (cmdActiveIndex + 1) % cmdMatches.count; return .handled }
                return .ignored
            }
            .onKeyPress(.upArrow) {
                if showRepoBarSuggest { cmdActiveIndex = (cmdActiveIndex - 1 + repoMatches.count) % repoMatches.count; return .handled }
                if showCmdBarSuggest { cmdActiveIndex = (cmdActiveIndex - 1 + cmdMatches.count) % cmdMatches.count; return .handled }
                return .ignored
            }
            .onKeyPress(.tab) {
                if showRepoBarSuggest { selectRepoFromBar(repoMatches[cmdActiveIndex].token); return .handled }
                if showCmdBarSuggest { selectCommandFromBar(cmdMatches[cmdActiveIndex].token); return .handled }
                return .ignored
            }
            .onKeyPress(.return) {
                if showRepoBarSuggest { selectRepoFromBar(repoMatches[cmdActiveIndex].token); return .handled }
                if showCmdBarSuggest { selectCommandFromBar(cmdMatches[cmdActiveIndex].token); return .handled }
                return .ignored
            }
            .onKeyPress(.escape) {
                if showRepoBarSuggest || showCmdBarSuggest { cmdDismissed = true; return .handled }
                return .ignored
            }
            .padding(.horizontal, 10).padding(.vertical, 8)
            .background(Color.white.opacity(0.02))
            .overlay(RoundedRectangle(cornerRadius: 7).stroke(cmdBarFocused ? Konjo.stackViolet.opacity(0.5) : Konjo.line2, lineWidth: 1))
            .clipShape(RoundedRectangle(cornerRadius: 7))
            .background(
                GeometryReader { geo in
                    Color.clear
                        .onAppear { cmdBarHeight = geo.size.height }
                        .onChange(of: geo.size.height) { _, h in cmdBarHeight = h }
                }
            )
            .overlay(alignment: .topLeading) {
                Group {
                    if showRepoBarSuggest { repoBarSuggestList }
                    else if showCmdBarSuggest { cmdBarSuggestList }
                }
                .offset(y: cmdBarHeight + 4)
            }
            .zIndex(showRepoBarSuggest || showCmdBarSuggest ? 10 : 0)
            .task(id: config.defaults.repo) { await model.ensureBranches(config.defaults.repo) }
    }

    private var repoBarSuggestList: some View {
        VStack(alignment: .leading, spacing: 2) {
            ForEach(Array(repoMatches.enumerated()), id: \.offset) { i, item in
                Button { selectRepoFromBar(item.token) } label: {
                    HStack(spacing: 8) {
                        Text(item.token).font(Konjo.mono(12, weight: .bold)).foregroundStyle(Konjo.stackViolet)
                        Text(item.label).font(Konjo.sans(11)).foregroundStyle(Konjo.fg)
                        Spacer(minLength: 8)
                        Text(item.hint).font(Konjo.mono(9)).foregroundStyle(Konjo.fgMute).lineLimit(1)
                    }
                    .padding(.horizontal, 8).padding(.vertical, 6)
                    .background(i == cmdActiveIndex ? Konjo.stackViolet.opacity(0.12) : Color.clear)
                    .clipShape(RoundedRectangle(cornerRadius: 5))
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
        }
        .padding(4)
        .frame(minWidth: 280)
        .background(Konjo.panel)
        .clipShape(RoundedRectangle(cornerRadius: 8))
        .overlay(RoundedRectangle(cornerRadius: 8).stroke(Color.white.opacity(0.11), lineWidth: 1))
        .shadow(color: .black.opacity(0.6), radius: 17, y: 8)
    }

    private var cmdBarSuggestList: some View {
        VStack(alignment: .leading, spacing: 2) {
            ForEach(Array(cmdMatches.enumerated()), id: \.offset) { i, item in
                Button { selectCommandFromBar(item.token) } label: {
                    HStack(spacing: 8) {
                        Text(item.token).font(Konjo.mono(12, weight: .bold)).foregroundStyle(Konjo.stackViolet)
                        Text(item.label).font(Konjo.sans(11)).foregroundStyle(Konjo.fg)
                        Spacer(minLength: 8)
                        Text(item.hint).font(Konjo.mono(9)).foregroundStyle(Konjo.fgMute).lineLimit(1)
                    }
                    .padding(.horizontal, 8).padding(.vertical, 6)
                    .background(i == cmdActiveIndex ? Konjo.stackViolet.opacity(0.12) : Color.clear)
                    .clipShape(RoundedRectangle(cornerRadius: 5))
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
        }
        .padding(4)
        .frame(minWidth: 280)
        .background(Konjo.panel)
        .clipShape(RoundedRectangle(cornerRadius: 8))
        .overlay(RoundedRectangle(cornerRadius: 8).stroke(Color.white.opacity(0.11), lineWidth: 1))
        .shadow(color: .black.opacity(0.6), radius: 17, y: 8)
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

    /// Web's `.runchev` sets no vertical padding of its own — it relies on
    /// `.runsplit`'s flex-row `align-items: stretch` default to inherit
    /// `.runmain`'s full height. SwiftUI's `HStack` doesn't stretch children
    /// that way, and `frame(maxHeight: .infinity)` overcorrects: it fills
    /// every bit of *unbounded* vertical space an ancestor offers (the whole
    /// scroll column here), not just the sibling's height. Reading
    /// `.runmain`'s actual rendered height via this preference key and
    /// applying it as a fixed `.frame(height:)` on the chevron is the
    /// measure-then-match approach that avoids both failure modes.
    private struct RunMainHeightKey: PreferenceKey {
        static var defaultValue: CGFloat = 0
        static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) {
            value = max(value, nextValue())
        }
    }

    private var runSplit: some View {
        HStack(spacing: 0) {
            Button(action: runMain) {
                HStack(spacing: 9) {
                    Image(systemName: phase == .running ? "pause.fill" : "play.fill").font(.system(size: 13, weight: .bold))
                    Text(runLabel).font(Konjo.sans(13, weight: .bold))
                }
                .padding(.horizontal, 26).padding(.vertical, 12)
                .background(LinearGradient(colors: [Color(hex: 0xFFB648), Konjo.flame], startPoint: .top, endPoint: .bottom))
                .foregroundStyle(Color(hex: 0x231000))
                .background(GeometryReader { geo in
                    Color.clear.preference(key: RunMainHeightKey.self, value: geo.size.height)
                })
            }
            .buttonStyle(.plain).disabled(phase == .draining)
            Button { runMenuOpen.toggle() } label: {
                Image(systemName: "chevron.up").font(.system(size: 12, weight: .bold)).foregroundStyle(Color(hex: 0x231000))
                    .padding(.horizontal, 13)
                    .frame(height: runMainHeight > 0 ? runMainHeight : nil)
                    .background(LinearGradient(colors: [Color(hex: 0xFFA733), Color(hex: 0xF08600)], startPoint: .top, endPoint: .bottom))
                    .overlay(Rectangle().fill(Color.black.opacity(0.28)).frame(width: 1), alignment: .leading)
            }
            .buttonStyle(.plain)
            .popover(isPresented: $runMenuOpen, arrowEdge: .top) {
                RunMenuView(store: store, engine: engine, paneKey: pane.key, defaults: defaults, phase: phase,
                            onDryRun: { dryRunResult = $0 }, onClose: { runMenuOpen = false })
            }
        }
        .onPreferenceChange(RunMainHeightKey.self) { runMainHeight = $0 }
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
            onToggle: {
                store.updateStackConfig(pane.key) { $0.scheduled.toggle() }
                model.syncStackSchedule(paneKey: pane.key, defaults: defaults)
            },
            onChange: { next in
                store.updateStackConfig(pane.key) { $0.cron = next }
                model.syncStackSchedule(paneKey: pane.key, defaults: defaults)
            })
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
    private var goalPopover: some View {
        GoalPopoverView(pursue: config.goal.pursue, noProgressLimit: config.goal.noProgressLimit, pursues: pursues,
            onTogglePursue: { store.updateStackConfig(pane.key) { $0.goal.pursue.toggle() } },
            onChangeNoProgressLimit: { n in store.updateStackConfig(pane.key) { $0.goal.noProgressLimit = n } })
    }
}
