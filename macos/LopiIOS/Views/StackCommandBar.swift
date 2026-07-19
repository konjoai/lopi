import SwiftUI
import LopiStacksKit

/// The stack dock — the "STACK" header, running total, collapse chevron,
/// the command bar, the stack-scope cardbar, and the "run stack" button.
/// Ported from the CURRENT web UI (`web/src/lib/components/stacks/
/// StackControlDock.svelte`), not the stale macOS SwiftUI view.
///
/// The command bar is the same trailing-token grammar as the card composer
/// (`:alias`, `@repo`, `/command/value`) but scoped to `STACK_COMMANDS` and
/// applied straight to `StackConfig` via `updateStackConfig` — there is no
/// commit step, each selection takes effect immediately, matching web.
struct StackDockView: View {
    @Environment(AppModel.self) private var model
    let paneKey: String

    @State private var dockOpen = true
    @State private var cmdText = ""
    @State private var popoverOpen = false
    @State private var popoverTab: CardFacet = .schedule

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

    var body: some View {
        VStack(alignment: .leading, spacing: 9) {
            dockHead
            if dockOpen {
                commandBar
                if !suggestions.isEmpty { suggestionStrip }

                HStack(spacing: 6) {
                    GrammarChip(label: ":alias", color: Konjo.stackTeal)
                    GrammarChip(label: "@repo", color: Konjo.ice)
                    GrammarChip(label: "/model", color: Konjo.stackViolet)
                    GrammarChip(label: "/effort", color: Konjo.flame)
                    GrammarChip(label: "×N", color: Konjo.sun)
                }

                cardBar

                Button {
                    model.stackEngine.runStack(paneKey, .run, PaneDefaults(config?.defaults ?? StackDefaults(
                        model: "", effort: "", repo: "", branch: "", autonomy: ""
                    )))
                } label: {
                    HStack(spacing: 6) {
                        Image(systemName: "play.fill").font(.system(size: 12))
                        Text("run stack").font(Konjo.sans(14, weight: .bold))
                    }
                    .foregroundStyle(Color(hex: 0x1A0F00))
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 12)
                    .background(
                        LinearGradient(colors: [Konjo.flame, Color(hex: 0xE6820A)], startPoint: .top, endPoint: .bottom),
                        in: RoundedRectangle(cornerRadius: 10)
                    )
                }
                .buttonStyle(.plain)
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
            Button { dockOpen.toggle() } label: {
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
                    Button(action: s.apply) {
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
            Button { popoverTab = .schedule; popoverOpen = true } label: {
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
        case "loop": return StackDockView.loopCountOptions
        case "branch":
            let repo = config?.defaults.repo ?? ""
            return (model.branchesByRepo[repo] ?? []).map { StackOption(value: $0, label: $0) }
        default: return []
        }
    }

    private static let loopCountOptions: [StackOption] = [
        StackOption(value: "0", label: "off"),
        StackOption(value: "1", label: "1"),
        StackOption(value: "2", label: "2"),
        StackOption(value: "3", label: "3"),
        StackOption(value: "5", label: "5"),
        StackOption(value: "10", label: "10")
    ]

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
            if let slashIndex = cmdText.lastIndex(of: "/") {
                cmdText.replaceSubrange(slashIndex..., with: "\(s.token)/")
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
            case "loop": cfg.loopCount = Int(value) ?? 0
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
