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
    @FocusState private var goalFocused: Bool

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
        store.commitDraft(paneKey)
        goalFocused = true
    }

    var body: some View {
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
        .background(Konjo.bg1.opacity(0.6))
        .overlay(
            RoundedRectangle(cornerRadius: 9)
                .stroke(borderColor, style: StrokeStyle(lineWidth: 1, dash: (isDraft && !hot) ? [4, 3] : []))
        )
        .clipShape(RoundedRectangle(cornerRadius: 9))
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
            ProvenanceChips(alias: card.alias, tpl: card.tpl, tplKind: card.tplKind)
            Spacer(minLength: 0)
        }
    }

    /// Goal on its own full-width inset line (a chip-adjacent field truncated in
    /// the mockup). Still honors `:alias @repo ×N` on commit via `finalizeDraft`.
    private var goalField: some View {
        TextField("describe the prompt or goal…  (:alias @repo ×N ok)", text: goalBinding)
            .textFieldStyle(.plain).font(Konjo.mono(14)).foregroundStyle(Konjo.fg)
            .focused($goalFocused)
            .onSubmit { commit() }
            .padding(.horizontal, 11).padding(.vertical, 9)
            .background(Color.white.opacity(0.02))
            .overlay(RoundedRectangle(cornerRadius: 7).stroke(goalFocused ? Konjo.stackTeal.opacity(0.4) : Konjo.line2, lineWidth: 1))
            .clipShape(RoundedRectangle(cornerRadius: 7))
            .padding(.top, 10)
    }

    private var goalBinding: Binding<String> {
        Binding(get: { card.goal }, set: { v in store.updateDraftInPane(paneKey) { $0.goal = v } })
    }

    private var committedSpec: some View {
        HStack(spacing: 9) {
            ProvenanceChips(alias: card.alias, tpl: card.tpl, tplKind: card.tplKind)
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
                CardbarButton(systemImage: "line.3.horizontal", help: "drag to reorder") {}
                CardbarButton(systemImage: "trash", accent: Konjo.rose, danger: true, help: "delete") { store.removeFromPane(paneKey, card.id) }
            }
        }
        .padding(.top, 12)
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
