import SwiftUI
import LopiStacksKit

/// StackCardView — one loop in the stack, and (Creation-Flow-1) the pane's
/// pre-commit **draft** card, via a `draft` branch in this *one* view rather
/// than a forked `DraftCardView`. Built *around* the same agent rendering the
/// Forge pane uses (`KonjoOrb` + `TranscriptView`, driven by the live agent
/// keyed on `card.taskId`). Wrapped with the cardbar (iteration pill · schedule
/// · guards · evals+count · config · then duplicate/drag/delete — or, on a
/// draft, a single `+ add`), the hide-inactive summary lines, and the inline
/// config drawer — matching web's `StackCard`. All mutation goes through
/// `StackStore` (a draft edits the pane's `draft`; a committed card edits itself).
struct StackCardView: View {
    @Environment(AppModel.self) private var model
    var store: StackStore
    var paneKey: String
    var card: StackCard
    var index: Int
    var paneDefaults: StackDefaults
    var repoOptions: [StackOption]
    var scheduleGoverned: Bool

    @State private var cfgOpen = false
    @State private var schedOpen = false
    @State private var guardOpen = false
    @State private var evalOpen = false
    @State private var cardHeight: CGFloat = 0
    @State private var dragArmed = false
    @FocusState private var goalFocused: Bool

    // ── alias autocomplete (`:token`) ────────────────────────────────────────
    @State private var aliasActiveIndex = 0
    @State private var aliasDismissed = false
    private var aliasMatches: [AliasSuggestion] { aliasAutocomplete(card.goal) }
    private var showAliasSuggest: Bool { isDraft && goalFocused && !aliasDismissed && !aliasMatches.isEmpty }

    // ── repo autocomplete (`@token`) ─────────────────────────────────────────
    // Independent dismiss/active state from the alias list since the two can
    // never be active at once — one requires a leading `:`, the other a
    // trailing `@`.
    @State private var repoActiveIndex = 0
    @State private var repoDismissed = false
    private var repoMatches: [RepoSuggestion] { repoAutocomplete(card.goal, repoOptions) }
    private var showRepoSuggest: Bool { isDraft && goalFocused && !repoDismissed && !repoMatches.isEmpty }
    /// The goal field box's own measured height (border box, before the outer
    /// `.padding(.top, 10)`) — the suggestion overlay offsets down by this much
    /// plus a small gap, so it sits flush under the input like the web
    /// `AutocompleteSuggest` dropdown instead of a native `.popover` bubble.
    @State private var goalFieldHeight: CGFloat = 40
    /// The provenance chip's label — reverse-looked-up from the resolved path
    /// so the chip survives even though `@token` is stripped from the goal
    /// text on commit (see `selectRepo`'s doc comment).
    private var cardRepoLabel: String? {
        card.config.repo.map { repoLabelForPath($0, repoOptions) }
    }

    // ── inline `/command` autocomplete (model/effort/branch/autonomy/eval/
    //    guard/schedule) ────────────────────────────────────────────────────
    // Two-level grammar, mirroring the user's own suggested `/model/<value>`
    // syntax: typing `/` suggests command names (`commandAutocomplete`);
    // picking a value-picker command moves into a second `/command/value`
    // token (`commandValueAutocomplete`) against that command's own catalog.
    // Picking a non-value-picker command (guard/schedule) fires immediately —
    // strips the token and flips the existing `guardOpen`/`schedOpen` state,
    // same as clicking its cardbar icon. No `/maxx` here — macOS `StackCard`
    // has no MAXX field yet (web-only feature to date).
    @State private var cmdActiveIndex = 0
    @State private var cmdDismissed = false
    /// Set once a value-picker command is chosen from the level-1 list.
    @State private var pendingCommand: String?

    /// One suggestion row, whichever level produced it — Swift has no union
    /// return type, so this wraps `CommandSuggestion`/`CommandValueSuggestion`
    /// behind one shape the view can render uniformly.
    private enum CmdMatch {
        case command(CommandSuggestion)
        case value(CommandValueSuggestion)
        var token: String { switch self { case .command(let c): return c.token; case .value(let v): return v.token } }
        var label: String { switch self { case .command(let c): return c.label; case .value(let v): return v.label } }
        var hint: String { switch self { case .command(let c): return c.hint; case .value(let v): return v.hint } }
    }

    private var effectiveRepo: String { card.config.repo ?? paneDefaults.repo }

    private func commandOptionsFor(_ command: String) -> [StackOption] {
        switch command {
        case "model": return MODEL_OPTIONS
        case "effort": return EFFORT_OPTIONS
        case "autonomy": return AUTONOMY_OPTIONS
        case "branch": return (model.branchesByRepo[effectiveRepo] ?? []).map { StackOption(value: $0, label: $0) }
        case "eval": return evalSuiteOptions()
        default: return []
        }
    }

    private var cmdMatches: [CmdMatch] {
        if let pendingCommand {
            return commandValueAutocomplete(card.goal, pendingCommand, commandOptionsFor(pendingCommand)).map { .value($0) }
        }
        return commandAutocomplete(card.goal, CARD_COMMANDS).map { .command($0) }
    }
    private var showCmdSuggest: Bool { isDraft && goalFocused && !cmdDismissed && !cmdMatches.isEmpty }

    private var isDraft: Bool { card.status == .draft }
    private var hot: Bool { isDraft && draftIsHot(card) }

    private var liveAgent: LiveAgent? { card.taskId.flatMap { model.liveAgents[$0] } }
    private var orb: ForgeOrbState { CardOrb.state(for: card.taskId, in: model.liveAgents) }
    private var guardsOn: Bool { guardActive(card.guardrails) }
    private var evalsOn: Bool { evalActive(card) }
    private var configOn: Bool { configActive(card, paneDefaults) }
    private var scheduleActive: Bool { card.scheduled && !scheduleGoverned }
    private var showSep: Bool { card.scheduled || guardsOn || evalsOn }

    /// Route a card mutation to the right store op: a draft edits the pane's
    /// `draft`; a committed card edits itself in `pane.cards`.
    private func writeCard(_ mutate: (inout StackCard) -> Void) {
        if isDraft { store.updateDraftInPane(paneKey, mutate) }
        else { store.updateCardInPane(paneKey, card.id, mutate) }
    }

    /// Commit the draft: mints a real card at the top of the stack and a fresh
    /// empty draft, then re-focuses the (now-empty) goal field for rapid entry.
    private func commit() {
        guard hot else { return }
        store.commitDraft(paneKey, repoOptions: repoOptions)
        goalFocused = true
    }

    var body: some View {
        if isDraft {
            cardContent
        } else {
            draggableCardContent
                .dropDestination(for: CardDragPayload.self) { items, location in
                    guard let payload = items.first, payload.paneKey == paneKey, payload.index != index else { return false }
                    let before = location.y < cardHeight / 2
                    store.reorderInPaneRelative(paneKey, payload.index, index, before)
                    return true
                }
                .background(GeometryReader { geo in
                    Color.clear.preference(key: CardHeightKey.self, value: geo.size.height)
                })
                .onPreferenceChange(CardHeightKey.self) { cardHeight = $0 }
        }
    }

    /// Mirrors web's `armDrag`/`disarmDrag` (`StackCard.svelte`): the whole
    /// card becomes a drag source, but only for the duration the drag handle
    /// is actually pressed. `.draggable()` can't be conditionally toggled by
    /// a flag directly — attaching/detaching it via an `if` branch on
    /// `dragArmed` is the SwiftUI equivalent of web's `draggable={armed}`
    /// HTML attribute. Without this, `.draggable()` permanently on
    /// `cardContent` would compete with every button/text field inside it
    /// for the press gesture (see `cardDragHandle`'s doc comment) — but
    /// putting `.draggable()` on the handle ALONE only made that small icon
    /// draggable, not the card, which is the wrong visual (confirmed by
    /// screen recording).
    @ViewBuilder
    private var draggableCardContent: some View {
        if dragArmed {
            cardContent.draggable(CardDragPayload(paneKey: paneKey, index: index))
        } else {
            cardContent
        }
    }

    private struct CardHeightKey: PreferenceKey {
        static var defaultValue: CGFloat = 0
        static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) { value = nextValue() }
    }

    private var cardContent: some View {
        VStack(alignment: .leading, spacing: 0) {
            agentBody
            summaryLines
            cardbar
            if cfgOpen {
                ConfigDrawerView(config: card.config, paneDefaults: paneDefaults, repoOptions: repoOptions) { next in
                    writeCard { $0.config = next }
                }
            }
        }
        .padding(13)
        // Fill *and* border both live in `.background()`, not `.overlay()` —
        // a card-wide `.clipShape` or border `.overlay()` would paint in
        // front of the goal field's `:`/`@`/`/` suggestion dropdown (itself
        // an `.overlay` nested inside this VStack), either clipping it at
        // the card's bottom edge or drawing the border stroke across it.
        // `.background()` content always paints behind a view's own
        // content, so the dropdown stays on top regardless of its zIndex.
        .background(
            RoundedRectangle(cornerRadius: 9)
                .fill(Konjo.bg1.opacity(0.6))
                .overlay(
                    RoundedRectangle(cornerRadius: 9)
                        .stroke(borderColor, style: StrokeStyle(lineWidth: 1, dash: (isDraft && !hot) ? [4, 3] : []))
                )
        )
        .overlay(alignment: .topTrailing) { runtag }
    }

    private var borderColor: Color {
        if isDraft { return hot ? Konjo.stackTeal.opacity(0.55) : Color.white.opacity(0.18) }
        switch card.status {
        case .running: return orb.glowColor.opacity(0.45)
        case .queued: return orb.glowColor.opacity(0.4)
        case .done: return orb.glowColor.opacity(0.35)
        case .idle, .draft: return Konjo.line
        }
    }

    // MARK: Status runtag badge (the mockup's `.runtag`, top-right)

    private var statusLabel: String {
        if isDraft { return "new prompt" }
        if card.status == .running, let it = card.iteration {
            return "running · iter \(it.current)/\(it.total)"
        }
        return card.status.rawValue
    }

    private var statusColor: Color {
        if isDraft { return hot ? Konjo.stackTeal : Konjo.fgDim }
        switch card.status {
        case .running: return Konjo.flame
        case .queued: return Konjo.ice
        case .done: return Konjo.jade
        case .idle, .draft: return Konjo.fgDim
        }
    }

    private var runtag: some View {
        HStack(spacing: 5) {
            if card.status == .running {
                Circle().fill(Konjo.flame).frame(width: 5, height: 5)
                    .shadow(color: Konjo.ember, radius: 3)
            }
            Text(statusLabel.uppercased()).font(Konjo.mono(9, weight: .medium)).tracking(1)
        }
        .foregroundStyle(statusColor)
        .padding(.horizontal, 8).padding(.vertical, 2)
        .background(Konjo.bg)
        .overlay(RoundedRectangle(cornerRadius: 3).stroke(statusColor.opacity((card.status == .idle || (isDraft && !hot)) ? 0.2 : 0.5), lineWidth: 1))
        .clipShape(RoundedRectangle(cornerRadius: 3))
        .offset(x: -14, y: -10)
        .help(isDraft ? "new prompt — configure, then add" : CardOrb.label(for: card))
        .allowsHitTesting(false)
    }

    // MARK: Body — draft (templates + goal field) or committed (spec + live agent)

    @ViewBuilder private var agentBody: some View {
        if isDraft {
            draftHeader
            goalField
        } else {
            committedSpec
            if card.status == .running, let it = card.iteration {
                iterBar(it)
            }
            if let agent = liveAgent, card.status == .running {
                LiveOutputView(blocks: TranscriptBuilder.build(from: agent), streaming: agent.active)
            }
        }
    }

    private var draftHeader: some View {
        HStack(spacing: 9) {
            TemplatesMenuView(store: store, templateStore: model.stackTemplateStore, paneKey: paneKey, card: card)
            ProvenanceChips(alias: card.alias, tpl: card.tpl, tplKind: card.tplKind, repoLabel: cardRepoLabel)
            Spacer(minLength: 0)
        }
    }

    /// Goal on its own full-width inset line (a chip-adjacent field truncated in
    /// the mockup). Still honors `:alias @repo ×N` on commit via `finalizeDraft`.
    private var goalField: some View {
        TextField("describe the prompt or goal...  (i.e. :alias @org/repo /model/opus xN)", text: goalBinding)
            .textFieldStyle(.plain).font(Konjo.mono(14)).foregroundStyle(Konjo.fg)
            .focused($goalFocused)
            .accessibilityIdentifier("stack.goalField")
            .onSubmit {
                if showAliasSuggest { selectAlias(aliasMatches[aliasActiveIndex].alias) }
                else if showRepoSuggest { selectRepo(repoMatches[repoActiveIndex].token) }
                else if showCmdSuggest { selectCommand(cmdMatches[cmdActiveIndex].token) }
                else { commit() }
            }
            .onChange(of: card.goal) { _, newGoal in
                aliasDismissed = false; aliasActiveIndex = 0
                repoDismissed = false; repoActiveIndex = 0
                cmdDismissed = false
                // Re-infer `pendingCommand` from the goal text on every
                // change, not just from `selectCommand`'s explicit
                // assignment — otherwise hand-typing `/model/` (rather than
                // clicking the `/model` row) never entered value-picker
                // mode. Falls back to the old clear-on-abandon behavior once
                // the `/command/` prefix itself is edited away (e.g.
                // backspaced).
                if let inferred = detectPendingCommand(newGoal, CARD_COMMANDS) {
                    pendingCommand = inferred
                } else if let pending = pendingCommand, !newGoal.contains("/\(pending)/") {
                    pendingCommand = nil
                }
            }
            .onKeyPress(.downArrow) {
                if showAliasSuggest { aliasActiveIndex = (aliasActiveIndex + 1) % aliasMatches.count; return .handled }
                if showRepoSuggest { repoActiveIndex = (repoActiveIndex + 1) % repoMatches.count; return .handled }
                if showCmdSuggest { cmdActiveIndex = (cmdActiveIndex + 1) % cmdMatches.count; return .handled }
                return .ignored
            }
            .onKeyPress(.upArrow) {
                if showAliasSuggest { aliasActiveIndex = (aliasActiveIndex - 1 + aliasMatches.count) % aliasMatches.count; return .handled }
                if showRepoSuggest { repoActiveIndex = (repoActiveIndex - 1 + repoMatches.count) % repoMatches.count; return .handled }
                if showCmdSuggest { cmdActiveIndex = (cmdActiveIndex - 1 + cmdMatches.count) % cmdMatches.count; return .handled }
                return .ignored
            }
            .onKeyPress(.tab) {
                if showAliasSuggest { selectAlias(aliasMatches[aliasActiveIndex].alias); return .handled }
                if showRepoSuggest { selectRepo(repoMatches[repoActiveIndex].token); return .handled }
                if showCmdSuggest { selectCommand(cmdMatches[cmdActiveIndex].token); return .handled }
                return .ignored
            }
            .onKeyPress(.escape) {
                if showAliasSuggest { aliasDismissed = true; return .handled }
                if showRepoSuggest { repoDismissed = true; return .handled }
                if showCmdSuggest { cmdDismissed = true; return .handled }
                return .ignored
            }
            .padding(.horizontal, 11).padding(.vertical, 9)
            .background(Color.white.opacity(0.02))
            .overlay(RoundedRectangle(cornerRadius: 7).stroke(goalFocused ? Konjo.stackTeal.opacity(0.4) : Konjo.line2, lineWidth: 1))
            .clipShape(RoundedRectangle(cornerRadius: 7))
            .background(
                GeometryReader { geo in
                    Color.clear
                        .onAppear { goalFieldHeight = geo.size.height }
                        .onChange(of: geo.size.height) { _, h in goalFieldHeight = h }
                }
            )
            // A flat, borderless dropdown flush under the input — matching the
            // web `AutocompleteSuggest` (`position: absolute; top: calc(100% +
            // 4px)`) — rather than SwiftUI's native `.popover`, which renders as
            // a system bubble with an arrow/tail and vibrancy chrome. Dismissal
            // on outside click needs no extra wiring: `showAliasSuggest`/
            // `showRepoSuggest`/`showCmdSuggest` already require `goalFocused`,
            // and clicking away clears focus, same as web's `on:blur`.
            .overlay(alignment: .topLeading) {
                Group {
                    if showAliasSuggest { aliasSuggestList }
                    else if showRepoSuggest { repoSuggestList }
                    else if showCmdSuggest { cmdSuggestList }
                }
                .offset(y: goalFieldHeight + 4)
            }
            .zIndex(showAliasSuggest || showRepoSuggest || showCmdSuggest ? 10 : 0)
            .padding(.top, 10)
            .task(id: effectiveRepo) { await model.ensureBranches(effectiveRepo) }
    }

    private var goalBinding: Binding<String> {
        Binding(get: { card.goal }, set: { v in store.updateDraftInPane(paneKey) { $0.goal = v } })
    }

    /// Replace the `:token` being typed with the full canonical alias plus a
    /// trailing space, so the cursor lands ready to type the goal text next —
    /// the popover closes itself since the goal no longer matches a bare
    /// `:token` once the space is there. Also applies the preset's
    /// alias/evals to the draft immediately via `applyPreset` — mirroring
    /// `selectRepo`/`applyCommandValue`, which already write their resolved
    /// facet onto `card`/`card.config` at selection time rather than waiting
    /// for commit. Without this the provenance chip (`card.alias`) never
    /// appeared and the preset's eval suite never attached until commit.
    private func selectAlias(_ alias: String) {
        let key = resolvePresetAlias(String(alias.dropFirst()))
        store.updateDraftInPane(paneKey) { c in
            if let key { c = applyPreset(key, to: c) }
            c.goal = "\(alias) "
        }
        aliasActiveIndex = 0
        goalFocused = true
    }

    private var aliasSuggestList: some View {
        VStack(alignment: .leading, spacing: 2) {
            ForEach(Array(aliasMatches.enumerated()), id: \.offset) { i, item in
                Button { selectAlias(item.alias) } label: {
                    HStack(spacing: 8) {
                        Text(item.alias).font(Konjo.mono(12, weight: .bold)).foregroundStyle(Konjo.stackTeal)
                        Text(item.label).font(Konjo.sans(11)).foregroundStyle(Konjo.fg)
                        Spacer(minLength: 8)
                        Text(item.hint).font(Konjo.mono(9)).foregroundStyle(Konjo.fgMute).lineLimit(1)
                    }
                    .padding(.horizontal, 8).padding(.vertical, 6)
                    .background(i == aliasActiveIndex ? Konjo.stackTeal.opacity(0.09) : Color.clear)
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

    /// Replace the trailing `@token` with the full `@owner/name` token plus a
    /// trailing space (keeps the human-readable label visible while typing).
    /// Also writes the *resolved path* straight onto `card.config.repo` —
    /// never relies on `parseComposerInput` re-deriving it from the label
    /// text later, which is the mismatch that made the repo drawer silently
    /// show "auto". `repoAutocomplete` only ever matches the goal's last
    /// word, so the match is always anchored at the string's end — "replace
    /// the match" and "replace the string's tail" are the same slice-and-
    /// append, no cursor-position tracking needed.
    private func selectRepo(_ token: String) {
        guard let atIndex = card.goal.lastIndex(of: "@") else { return }
        let resolved = repoMatches.first(where: { $0.token == token })?.value
        store.updateDraftInPane(paneKey) {
            $0.goal = "\(card.goal[..<atIndex])\(token) "
            if let resolved { $0.config.repo = resolved }
        }
        repoActiveIndex = 0
        goalFocused = true
    }

    /// Apply a value-picker command's chosen value directly to `card.config`
    /// (or toggle the eval suite) — no chip; the existing config-gear/evals-
    /// count indicators already surface these once set.
    private func applyCommandValue(_ command: String, _ value: String) {
        store.updateDraftInPane(paneKey) { c in
            switch command {
            case "eval": c.evals = applySuite(c.evals, EVAL_SUITES[value] ?? [])
            case "model": c.config.model = value
            case "effort": c.config.effort = value
            case "branch": c.config.branch = value
            case "autonomy": c.config.autonomy = value
            default: break
            }
        }
    }

    /// Fire a non-value-picker command's immediate action — flips the same
    /// state its cardbar icon does.
    private func fireCommandAction(_ command: String) {
        if command == "guard" { guardOpen = true }
        else if command == "schedule" { schedOpen = true }
    }

    private func selectCommand(_ token: String) {
        if let pending = pendingCommand {
            if case .value(let suggestion)? = cmdMatches.first(where: { $0.token == token }) {
                let prefix = "/\(pending)/"
                if let range = card.goal.range(of: prefix, options: .backwards) {
                    store.updateDraftInPane(paneKey) { $0.goal = String(card.goal[..<range.lowerBound]) }
                    applyCommandValue(pending, suggestion.value)
                }
            }
            pendingCommand = nil
        } else {
            let command = String(token.dropFirst())
            guard let slashIndex = card.goal.lastIndex(of: "/") else { return }
            let def = CARD_COMMANDS.first(where: { $0.command == command })
            if def?.isValuePicker == true {
                store.updateDraftInPane(paneKey) { $0.goal = "\(card.goal[..<slashIndex])/\(command)/" }
                pendingCommand = command
            } else {
                store.updateDraftInPane(paneKey) { $0.goal = String(card.goal[..<slashIndex]) }
                fireCommandAction(command)
            }
        }
        cmdActiveIndex = 0
        goalFocused = true
    }

    private var cmdSuggestList: some View {
        VStack(alignment: .leading, spacing: 2) {
            ForEach(Array(cmdMatches.enumerated()), id: \.offset) { i, item in
                Button { selectCommand(item.token) } label: {
                    HStack(spacing: 8) {
                        Text(item.token).font(Konjo.mono(12, weight: .bold)).foregroundStyle(Konjo.stackTeal)
                        Text(item.label).font(Konjo.sans(11)).foregroundStyle(Konjo.fg)
                        Spacer(minLength: 8)
                        Text(item.hint).font(Konjo.mono(9)).foregroundStyle(Konjo.fgMute).lineLimit(1)
                    }
                    .padding(.horizontal, 8).padding(.vertical, 6)
                    .background(i == cmdActiveIndex ? Konjo.stackTeal.opacity(0.09) : Color.clear)
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

    private var repoSuggestList: some View {
        VStack(alignment: .leading, spacing: 2) {
            ForEach(Array(repoMatches.enumerated()), id: \.offset) { i, item in
                Button { selectRepo(item.token) } label: {
                    HStack(spacing: 8) {
                        Text(item.token).font(Konjo.mono(12, weight: .bold)).foregroundStyle(Konjo.stackTeal)
                        Text(item.label).font(Konjo.sans(11)).foregroundStyle(Konjo.fg)
                        Spacer(minLength: 8)
                        Text(item.hint).font(Konjo.mono(9)).foregroundStyle(Konjo.fgMute).lineLimit(1)
                    }
                    .padding(.horizontal, 8).padding(.vertical, 6)
                    .background(i == repoActiveIndex ? Konjo.stackTeal.opacity(0.09) : Color.clear)
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

    private var committedSpec: some View {
        HStack(spacing: 9) {
            ProvenanceChips(alias: card.alias, tpl: card.tpl, tplKind: card.tplKind, repoLabel: cardRepoLabel)
            Text("\"\(card.goal)\"").font(Konjo.mono(14)).foregroundStyle(Konjo.fgDim)
            Spacer(minLength: 0)
        }
    }

    private func iterBar(_ it: IterationProgress) -> some View {
        HStack(spacing: 4) {
            ForEach(0..<max(it.total, 0), id: \.self) { i in
                RoundedRectangle(cornerRadius: 2)
                    .fill(i < it.current - 1 ? Konjo.jade : (i == it.current - 1 ? Konjo.flame : Color.white.opacity(0.11)))
                    .frame(width: 22, height: 3)
            }
        }
        .padding(.top, 9)
    }

    // MARK: Summary lines

    @ViewBuilder private var summaryLines: some View {
        if showSep {
            Divider().overlay(Konjo.line).padding(.top, 11)
            VStack(alignment: .leading, spacing: 8) {
                if card.scheduled {
                    SummaryRow(systemImage: "clock", label: "schedule", accent: scheduleGoverned ? Konjo.fgMute : FacetAccent.schedule,
                               text: scheduleGoverned ? "governed by stack — won't fire on its own" : scheduleSummary(card))
                }
                if guardsOn { SummaryRow(systemImage: "shield", label: "guards", accent: FacetAccent.guards, text: guardSummary(card)) }
                if evalsOn { SummaryRow(systemImage: "checkmark.square", label: "evals", accent: FacetAccent.evals, text: evalsSummary(card)) }
            }
            .padding(.top, 8)
        }
    }

    // MARK: Cardbar (live on the draft too — configure, then commit)

    private var cardbar: some View {
        HStack(spacing: 6) {
            IterationPill(value: card.maxIterations, offAtZero: true) { delta in
                writeCard { $0.maxIterations = stepCardIterations($0.maxIterations, delta) }
            }
            CardbarButton(systemImage: "clock", active: scheduleActive, accent: FacetAccent.schedule, help: scheduleGoverned ? "schedule (governed by the stack)" : "schedule") { schedOpen = true }
                .popover(isPresented: $schedOpen, arrowEdge: .bottom) { schedulePopover }
            CardbarButton(systemImage: "shield", active: guardsOn, accent: FacetAccent.guards, help: "guardrails") { guardOpen = true }
                .popover(isPresented: $guardOpen, arrowEdge: .bottom) { guardsPopover }
            CardbarButton(systemImage: "checkmark.square", active: evalsOn, accent: FacetAccent.evals, count: card.evals.count, help: "evals") { evalOpen = true }
                .popover(isPresented: $evalOpen, arrowEdge: .bottom) { evalsPopover }
            CardbarButton(systemImage: "slider.horizontal.3", active: configOn, accent: FacetAccent.config, help: "run config") { cfgOpen.toggle() }
            Spacer()
            if isDraft {
                CardbarButton(systemImage: "plus", active: hot, accent: Konjo.jade, label: "add", disabled: !hot, help: "add to stack") { commit() }
            } else {
                TemplatesMenuView(store: store, templateStore: model.stackTemplateStore, paneKey: paneKey, card: card, isDraft: false)
                CardbarButton(systemImage: "square.on.square", help: "duplicate") { store.duplicateInPane(paneKey, card.id) }
                cardDragHandle
                CardbarButton(systemImage: "trash", accent: Konjo.rose, danger: true, help: "delete") { store.removeFromPane(paneKey, card.id) }
            }
        }
        .padding(.top, 12)
    }

    /// Same visual chrome as `CardbarButton`, deliberately NOT a `Button`
    /// (a `.draggable()`/gesture chained onto an actual `Button` loses to
    /// the button's own tap recognizer — see `StackControlDockView.dragHandle`).
    /// Doesn't carry `.draggable()` itself — it only arms/disarms
    /// `dragArmed`, which `draggableCardContent` uses to attach
    /// `.draggable()` to the WHOLE card for exactly the press's duration,
    /// mirroring web's `armDrag`/`disarmDrag` (`on:mousedown`/`on:mouseup`
    /// toggling the card's own `draggable` HTML attribute). Putting
    /// `.draggable()` on the handle alone only made the small icon
    /// draggable, not the card.
    private var cardDragHandle: some View {
        Image(systemName: "line.3.horizontal").font(.system(size: 12))
            .padding(.horizontal, 7)
            .frame(minWidth: 29, minHeight: 29)
            .foregroundStyle(Konjo.fgMute)
            .overlay(RoundedRectangle(cornerRadius: 6).stroke(Konjo.line, lineWidth: 1))
            .contentShape(Rectangle())
            .help("drag to reorder")
            .accessibilityIdentifier("drag to reorder")
            .gesture(
                DragGesture(minimumDistance: 0, coordinateSpace: .local)
                    .onChanged { _ in dragArmed = true }
                    .onEnded { _ in dragArmed = false }
            )
    }

    private var schedulePopover: some View {
        SchedulePopoverView(scheduled: card.scheduled, cron: card.cron,
            onToggle: { writeCard { $0.scheduled.toggle() } },
            onChange: { next in writeCard { $0.cron = next } })
    }
    private var guardsPopover: some View {
        GuardrailsPopoverView(scope: .loop, guardrails: card.guardrails, maxIterations: card.maxIterations,
            onChange: { g in writeCard { $0.guardrails = g } },
            onStep: { delta in writeCard { $0.maxIterations = stepCardIterations($0.maxIterations, delta) } })
    }
    private var evalsPopover: some View {
        EvalsPopoverView(evals: card.evals) { evals in writeCard { $0.evals = evals } }
    }
}
