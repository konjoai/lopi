import SwiftUI
import LopiStacksKit

/// The stack dock — the "STACK" header, running total, collapse chevron,
/// the command bar, the stack-scope cardbar, and the "run stack" button.
/// Ported from the CURRENT web UI (`web/src/lib/components/stacks/
/// StackControlDock.svelte`), not the stale macOS SwiftUI view.
///
/// The command bar is the same trailing-token grammar as the card composer
/// (`:alias`, `@repo`, `;command/value`) but scoped to `STACK_COMMANDS` and
/// applied straight to `StackConfig` via `updateStackConfig` — there is no
/// commit step, each selection takes effect immediately, matching web.
struct StackDockView: View {
    @Environment(AppModel.self) private var model
    let paneKey: String

    @State private var dockOpen = true
    @State private var cmdText = ""
    @State private var popoverOpen = false
    @State private var popoverTab: CardFacet = .schedule
    @State private var runMenuOpen = false
    @State private var dryRunResult: DryRunResult?

    private var pane: StackPaneState? { model.stackStore.pane(for: paneKey) }
    private var config: StackConfig? { pane?.config }

    private var runningTotal: Double {
        (pane?.cards ?? []).compactMap { $0.taskId }
            .compactMap { model.liveAgents[$0] }
            .reduce(0.0) { $0 + $1.costUsd }
    }

    private var isRunning: Bool {
        (pane?.cards ?? []).compactMap { $0.taskId }
            .compactMap { model.liveAgents[$0] }
            .contains(where: \.active)
    }

    private var runState: StackRunState? { model.stackEngine.run(for: paneKey) }
    private var phase: RunPhase? { runState?.phase }
    private var runDefaults: PaneDefaults {
        PaneDefaults(config?.defaults ?? StackDefaults(model: "", effort: "", repo: "", branch: "", autonomy: ""))
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 9) {
            dockHead
            if dockOpen {
                commandBar
                if !suggestions.isEmpty { suggestionStrip }

                HStack(spacing: 6) {
                    GrammarChip(label: ":alias", color: Konjo.stackTeal)
                    GrammarChip(label: "@repo", color: Konjo.ice)
                    GrammarChip(label: ";model", color: Konjo.stackViolet)
                    GrammarChip(label: ";effort", color: Konjo.flame)
                    GrammarChip(label: "×N", color: Konjo.sun)
                }

                cardBar
                runStatusBanner
                runSplit
            }
        }
        .padding(.horizontal, 16)
        .padding(.top, 12)
        .padding(.bottom, 16)
        .background(
            LinearGradient(colors: [Konjo.violet.opacity(0.12), Konjo.violet.opacity(0.04)], startPoint: .top, endPoint: .bottom)
        )
        .overlay(alignment: .top) { Rectangle().fill(Konjo.violet.opacity(0.3)).frame(height: 1) }
        .onChange(of: isRunning) { _, running in
            if running { dockOpen = false }
        }
    }

    private var dockHead: some View {
        HStack(spacing: 8) {
            Text("STACK")
                .font(Konjo.mono(9, weight: .bold))
                .tracking(0.5)
                .foregroundStyle(.white)
                .padding(.horizontal, 7).padding(.vertical, 2)
                .background(Konjo.violet, in: RoundedRectangle(cornerRadius: 5))
            Text("running total: ")
                .font(Konjo.mono(10.5)).foregroundStyle(Konjo.fgDim)
            + Text(String(format: "$%.2f", runningTotal))
                .font(Konjo.mono(10.5, weight: .bold)).foregroundStyle(Konjo.fg)
            Spacer()
            Button { Haptics.tap(); dockOpen.toggle() } label: {
                Image(systemName: "chevron.down")
                    .font(.system(size: 11))
                    .foregroundStyle(Konjo.fgMute)
                    .rotationEffect(.degrees(dockOpen ? 0 : -90))
            }
            .buttonStyle(.plain)
        }
    }

    private var commandBar: some View {
        TextField("stack command…", text: $cmdText)
            .font(Konjo.sans(12.5))
            .foregroundStyle(Konjo.fg)
            .padding(.horizontal, 11).padding(.vertical, 9)
            .frame(maxWidth: .infinity, alignment: .leading)
            .overlay(RoundedRectangle(cornerRadius: 8).stroke(Konjo.violet.opacity(0.3), lineWidth: 1))
            .autocorrectionDisabled()
            .textInputAutocapitalization(.never)
            .onSubmit { suggestions.first?.apply() }
    }

    private var suggestionStrip: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 6) {
                ForEach(suggestions) { s in
                    Button(action: { Haptics.selection(); s.apply() }) {
                        VStack(alignment: .leading, spacing: 1) {
                            Text(s.label).font(Konjo.mono(10.5, weight: .semibold)).foregroundStyle(Konjo.ice)
                            if !s.hint.isEmpty {
                                Text(s.hint).font(Konjo.mono(8.5)).foregroundStyle(Konjo.fgMute)
                            }
                        }
                        .padding(.horizontal, 8).padding(.vertical, 5)
                        .background(Color.white.opacity(0.04), in: RoundedRectangle(cornerRadius: 6))
                        .overlay(RoundedRectangle(cornerRadius: 6).stroke(Konjo.ice.opacity(0.3), lineWidth: 1))
                    }
                    .buttonStyle(.plain)
                }
            }
        }
    }

    private var cardBar: some View {
        HStack(spacing: 6) {
            IterationPill(label: (config?.loopCount ?? 0) == 0 ? "off" : "×\(config?.loopCount ?? 0)")
            Button { Haptics.tap(); popoverTab = .schedule; popoverOpen = true } label: {
                Text("•••")
                    .font(Konjo.mono(10.5))
                    .foregroundStyle(Konjo.fgDim)
                    .padding(.horizontal, 10)
                    .frame(height: 26)
                    .overlay(RoundedRectangle(cornerRadius: 6).stroke(Konjo.line2, lineWidth: 1))
            }
            .buttonStyle(.plain)
            .popover(isPresented: $popoverOpen) {
                FacetPopoverContent(paneKey: paneKey, card: nil, isDraft: false, initialTab: popoverTab)
                    .presentationCompactAdaptation(.popover)
            }
            Spacer()
        }
    }

    // MARK: - Run controls (Run now / Run once / Schedule / Dry run, Pause/Resume + Drain)

    @ViewBuilder private var runStatusBanner: some View {
        if let stopReason = runState?.stopReason {
            runBanner(stackStopLabel(stopReason), ok: stopReason == .goalMet) { model.stackEngine.clearRun(paneKey) }
        } else if let runError = runState?.error {
            runBanner(runError, ok: false) { model.stackEngine.clearRun(paneKey) }
        } else if let dryRunResult {
            runBanner(dryRunText(dryRunResult), ok: dryRunResult.valid) { self.dryRunResult = nil }
        }
    }

    private var runSplit: some View {
        HStack(spacing: 0) {
            Button(action: runMain) {
                HStack(spacing: 6) {
                    Image(systemName: phase == .running ? "pause.fill" : "play.fill").font(.system(size: 12))
                    Text(runLabel).font(Konjo.sans(14, weight: .bold))
                }
                .foregroundStyle(Color(hex: 0x1A0F00))
                .frame(maxWidth: .infinity)
                .padding(.vertical, 12)
                .background(LinearGradient(colors: [Konjo.flame, Color(hex: 0xE6820A)], startPoint: .top, endPoint: .bottom))
            }
            .buttonStyle(.plain)
            .disabled(phase == .draining)
            Button { Haptics.tap(); runMenuOpen = true } label: {
                Image(systemName: "chevron.up").font(.system(size: 12, weight: .bold))
                    .foregroundStyle(Color(hex: 0x1A0F00))
                    .padding(.horizontal, 14)
                    .frame(maxHeight: .infinity)
                    .background(LinearGradient(colors: [Color(hex: 0xE6820A), Color(hex: 0xC96C00)], startPoint: .top, endPoint: .bottom))
                    .overlay(Rectangle().fill(Color.black.opacity(0.22)).frame(width: 1), alignment: .leading)
            }
            .buttonStyle(.plain)
        }
        .frame(height: 46)
        .clipShape(RoundedRectangle(cornerRadius: 10))
        .popover(isPresented: $runMenuOpen) {
            StackRunMenuContent(paneKey: paneKey, defaults: runDefaults, phase: phase,
                onDryRun: { dryRunResult = $0 }, onClose: { runMenuOpen = false })
                .presentationCompactAdaptation(.popover)
        }
    }

    private var runLabel: String {
        switch phase {
        case .running: return "pause"
        case .paused: return "resume"
        case .draining: return "draining…"
        default: return "run stack"
        }
    }

    private func runMain() {
        Haptics.impact()
        switch phase {
        case .running: model.stackEngine.pauseStack(paneKey)
        case .paused: model.stackEngine.resumeStack(paneKey, runDefaults)
        default: dryRunResult = nil; model.stackEngine.runStack(paneKey, .run, runDefaults)
        }
        runMenuOpen = false
    }

    private func dryRunText(_ r: DryRunResult) -> String {
        if r.valid { return "dry run: \(r.plan.count) loop\(r.plan.count == 1 ? "" : "s") would run, in order" }
        return "dry run found \(r.issues.count) issue\(r.issues.count == 1 ? "" : "s"): \(r.issues.first?.message ?? "")"
    }

    private func runBanner(_ text: String, ok: Bool, dismiss: @escaping () -> Void) -> some View {
        HStack(spacing: 8) {
            Text(text).font(Konjo.mono(10.5)).foregroundStyle(ok ? Konjo.jade : (text.hasPrefix("dry run:") ? Konjo.fgDim : Konjo.rose)).lineLimit(2)
            Spacer(minLength: 0)
            Button(action: { Haptics.tap(); dismiss() }) { Image(systemName: "xmark").font(.system(size: 10)).foregroundStyle(Konjo.fgDim) }.buttonStyle(.plain)
        }
        .padding(.horizontal, 10).padding(.vertical, 7)
        .background((ok ? Konjo.jade : Konjo.rose).opacity(0.1))
        .overlay(RoundedRectangle(cornerRadius: 7).stroke((ok ? Konjo.jade : Konjo.rose).opacity(0.35), lineWidth: 1))
        .clipShape(RoundedRectangle(cornerRadius: 7))
    }

    // MARK: - Command grammar

    private struct CmdSuggestion: Identifiable {
        let id: String
        let label: String
        let hint: String
        let apply: () -> Void
    }

    private var repoOpts: [StackOption] { repoOptions(model.repos) }

    private func valueOptions(_ command: String) -> [StackOption] {
        switch command {
        case "model": return MODEL_OPTIONS
        case "effort": return EFFORT_OPTIONS
        case "autonomy": return AUTONOMY_OPTIONS
        case "eval": return evalSuiteOptions()
        case "branch":
            let repo = config?.defaults.repo ?? ""
            return (model.branchesByRepo[repo] ?? []).map { StackOption(value: $0, label: $0) }
        default: return []
        }
    }

    private var suggestions: [CmdSuggestion] {
        if let command = detectPendingCommand(cmdText, STACK_COMMANDS) {
            return commandValueAutocomplete(cmdText, command, valueOptions(command)).map { s in
                CmdSuggestion(id: s.token, label: s.value, hint: s.label) {
                    applyCommand(command, s.value)
                    cmdText = ""
                }
            }
        }

        let aliasSugs = aliasAutocomplete(cmdText)
        if !aliasSugs.isEmpty {
            return aliasSugs.map { s in
                CmdSuggestion(id: s.alias, label: s.alias, hint: s.hint) {
                    applyAlias(s.alias)
                    cmdText = ""
                }
            }
        }

        let repoSugs = repoAutocomplete(cmdText, repoOpts)
        if !repoSugs.isEmpty {
            return repoSugs.map { s in
                CmdSuggestion(id: s.token, label: s.token, hint: s.hint) {
                    applyCommand("repo", s.value)
                    cmdText = ""
                }
            }
        }

        return commandAutocomplete(cmdText, STACK_COMMANDS).map { s in
            CmdSuggestion(id: s.token, label: s.token, hint: s.hint) { completeCommandToken(s) }
        }
    }

    private func completeCommandToken(_ s: CommandSuggestion) {
        let def = STACK_COMMANDS.first(where: { $0.command == s.command })
        if def?.isValuePicker == true {
            if let triggerIndex = cmdText.lastIndex(of: ";") {
                cmdText.replaceSubrange(triggerIndex..., with: "\(s.token)/")
            }
        } else {
            cmdText = ""
            popoverTab = s.command == "goal" ? .goal : s.command == "guard" ? .guardrails : .schedule
            popoverOpen = true
        }
    }

    private func applyAlias(_ alias: String) {
        let bare = alias.hasPrefix(":") ? String(alias.dropFirst()) : alias
        guard let key = resolvePresetAlias(bare), let def = PRESET_CATALOG[key] else { return }
        model.stackStore.updateStackConfig(paneKey) { $0.evals = def.evals }
    }

    private func applyCommand(_ command: String, _ value: String) {
        model.stackStore.updateStackConfig(paneKey) { cfg in
            switch command {
            case "model": cfg.defaults.model = value
            case "effort": cfg.defaults.effort = value
            case "branch": cfg.defaults.branch = value
            case "autonomy": cfg.defaults.autonomy = value
            case "repo": cfg.defaults.repo = value
            case "eval":
                for name in EVAL_SUITES[value] ?? [] {
                    guard let ref = EVAL_CATALOG.first(where: { $0.name == name }), !cfg.evals.contains(ref) else { continue }
                    cfg.evals.append(ref)
                }
            default: break
            }
        }
    }
}

/// The run-menu popover — Run now / Run once / Schedule stack / Dry run when
/// no run is active, or Pause/Resume + Drain once one is. Dry run stays
/// available in both states (it never touches execution). iOS analogue of
/// macOS's `RunMenuView`; reads `model.stackEngine`/`model.stackStore`
/// directly rather than taking them as params, matching this file's own
/// convention.
private struct StackRunMenuContent: View {
    @Environment(AppModel.self) private var model
    let paneKey: String
    let defaults: PaneDefaults
    let phase: RunPhase?
    let onDryRun: (DryRunResult) -> Void
    let onClose: () -> Void

    private struct Item: Identifiable {
        let id = UUID()
        let systemImage: String
        let name: String
        let sub: String
        let action: () -> Void
    }

    var body: some View {
        VStack(spacing: 0) {
            ForEach(items) { it in
                Button { Haptics.impact(); it.action(); onClose() } label: {
                    HStack(spacing: 12) {
                        Image(systemName: it.systemImage).font(.system(size: 14)).foregroundStyle(Konjo.flame).frame(width: 18)
                        Text(it.name).font(Konjo.sans(14)).foregroundStyle(Konjo.fg)
                        Spacer()
                        Text(it.sub).font(Konjo.mono(9.5)).foregroundStyle(Konjo.fgMute)
                    }
                    .padding(.horizontal, 16).padding(.vertical, 13)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                Divider().overlay(Konjo.line)
            }
        }
        .frame(width: 300)
        .background(Konjo.panel)
    }

    private var items: [Item] {
        var out: [Item] = []
        if phase == .running {
            out.append(Item(systemImage: "pause.fill", name: "Pause", sub: "halt after this card") { model.stackEngine.pauseStack(paneKey) })
            out.append(Item(systemImage: "xmark", name: "Drain", sub: "finish then stop") { model.stackEngine.drainStack(paneKey) })
        } else if phase == .paused {
            out.append(Item(systemImage: "play.fill", name: "Resume", sub: "continue run") { model.stackEngine.resumeStack(paneKey, defaults) })
            out.append(Item(systemImage: "xmark", name: "Drain", sub: "stop for good") { model.stackEngine.drainStack(paneKey) })
        } else {
            out.append(Item(systemImage: "play.fill", name: "Run now", sub: "start now") { model.stackEngine.runStack(paneKey, .run, defaults) })
            out.append(Item(systemImage: "checkmark", name: "Run once", sub: "one pass each") { model.stackEngine.runStack(paneKey, .runOnce, defaults) })
            out.append(Item(systemImage: "clock", name: "Schedule stack", sub: "schedule the entire stack") { scheduleStack() })
        }
        out.append(Item(systemImage: "flask", name: "Dry run", sub: "validate only") { dryRun() })
        return out
    }

    private func dryRun() {
        let cards = model.stackStore.pane(for: paneKey)?.cards ?? []
        onDryRun(dryRunStack(cards, defaults))
    }

    private func scheduleStack() {
        let cards = executionOrder(model.stackStore.pane(for: paneKey)?.cards ?? [])
        guard let first = cards.first else { return }
        let cronExpr = buildCronString(first.cron)
        Task { _ = await model.stackEngine.scheduleStack(paneKey, cronExpr, defaults) }
    }
}
