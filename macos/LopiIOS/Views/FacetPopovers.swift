import SwiftUI
import LopiStacksKit

/// The six facets a card (or, for `goal`, the whole stack) can configure —
/// the popover content behind each "•••" trigger in `StackDetailScreen.swift`.
/// Ported from the CURRENT web UI (`web/src/lib/components/stacks/
/// SchedulePopover.svelte`, `GuardrailsPopover.svelte`, `EvalsPopover.svelte`,
/// `GoalPopover.svelte`, `MaxxPopover.svelte`, `ConfigDrawer.svelte`) — the
/// macOS SwiftUI Forge views are stale and were deliberately NOT used as the
/// reference here.
///
/// `goal` is pane/stack-scoped on the web (there is no per-card goal facet;
/// `GoalPopover` is only ever mounted once, on the stack dock) — every card's
/// `goal` tab here reads/writes the same shared `StackConfig.goal`, matching
/// that behavior rather than inventing a new per-card field.

typealias CardMutator = (inout StackCard) -> Void

struct FacetPopoverContent: View {
    @Environment(AppModel.self) private var model
    let paneKey: String
    /// `nil` for the stack dock's popover, which has no single card in scope
    /// — only the pane-scoped `goal` tab is available there.
    let card: StackCard?
    let isDraft: Bool
    @State private var tab: CardFacet = .schedule

    private func write(_ mutate: @escaping CardMutator) {
        guard let card else { return }
        if isDraft { model.stackStore.updateDraftInPane(paneKey, mutate) }
        else { model.stackStore.updateCardInPane(paneKey, card.id, mutate) }
    }

    var body: some View {
        VStack(spacing: 0) {
            tabStrip
            Group {
                switch tab {
                case .schedule: cardScoped { ScheduleFacetView(card: $0, write: write) }
                case .guardrails: cardScoped { GuardrailsFacetView(card: $0, write: write) }
                case .evals: cardScoped { EvalsFacetView(card: $0, write: write) }
                case .goal: GoalFacetView(paneKey: paneKey)
                case .maxx: cardScoped { MaxxFacetView(card: $0, write: write) }
                case .config: cardScoped { ConfigFacetView(paneKey: paneKey, card: $0, write: write) }
                }
            }
        }
        .frame(width: 280)
        .frame(maxHeight: 420)
        .background(Color(hex: 0x101418))
    }

    @ViewBuilder
    private func cardScoped<Content: View>(@ViewBuilder _ content: (StackCard) -> Content) -> some View {
        if let card {
            content(card)
        } else {
            Text("select a loop card to configure this")
                .font(Konjo.sans(11.5)).foregroundStyle(Konjo.fgMute)
                .padding(14)
        }
    }

    private var tabStrip: some View {
        HStack(spacing: 0) {
            ForEach(CardFacet.allCases) { facet in
                Button { tab = facet } label: {
                    Image(systemName: facet.systemImage)
                        .font(.system(size: 13))
                        .foregroundStyle(tab == facet ? Konjo.fg : Konjo.fgMute)
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 8)
                        .background(tab == facet ? Color.white.opacity(0.06) : .clear)
                        .overlay(alignment: .bottom) {
                            Rectangle().fill(tab == facet ? Konjo.ice : .clear).frame(height: 2)
                        }
                }
                .buttonStyle(.plain)
            }
        }
        .overlay(alignment: .bottom) { Rectangle().fill(Konjo.line).frame(height: 1) }
    }
}

/// Matches `FacetAccent` (`Stacks/StackTheme.swift`) for color consistency.
enum CardFacet: String, CaseIterable, Identifiable {
    case schedule, guardrails, evals, goal, maxx, config

    var id: String { rawValue }
    var label: String { rawValue }

    var systemImage: String {
        switch self {
        case .schedule: return "clock"
        case .guardrails: return "checkmark.shield"
        case .evals: return "checklist"
        case .goal: return "target"
        case .maxx: return "bolt.fill"
        case .config: return "gearshape"
        }
    }
}

// MARK: - Schedule

private struct ScheduleFacetView: View {
    let card: StackCard
    let write: (@escaping CardMutator) -> Void

    private var cron: CronConfig { card.cron }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                HStack(spacing: 8) {
                    Toggle("", isOn: Binding(
                        get: { card.scheduled },
                        set: { v in write { $0.scheduled = v } }
                    )).labelsHidden().tint(Konjo.ice)
                    Text("run on a schedule").font(Konjo.mono(11.5)).foregroundStyle(Konjo.fg)
                }

                if card.scheduled {
                    Picker("frequency", selection: Binding(
                        get: { cron.freq }, set: { v in write { $0.cron.freq = v } }
                    )) {
                        ForEach(CronFreq.allCases, id: \.self) { Text($0.rawValue).tag($0) }
                    }
                    .pickerStyle(.menu).tint(Konjo.ice)

                    switch cron.freq {
                    case .weekly:
                        Picker("day", selection: Binding(
                            get: { cron.dow }, set: { v in write { $0.cron.dow = v } }
                        )) {
                            ForEach(Dow.allCases, id: \.self) { Text($0.rawValue).tag($0) }
                        }
                        .pickerStyle(.menu)
                        timeRow
                    case .daily:
                        timeRow
                    case .hourly:
                        Stepper("at :\(String(format: "%02d", cron.min))", value: Binding(
                            get: { cron.min }, set: { v in write { $0.cron.min = v } }
                        ), in: 0...59)
                        .font(Konjo.mono(11)).foregroundStyle(Konjo.fgDim)
                    case .everyMinute:
                        EmptyView()
                    case .custom:
                        TextField("raw cron", text: Binding(
                            get: { cron.raw }, set: { v in write { $0.cron.raw = v } }
                        ))
                        .font(Konjo.mono(11.5)).foregroundStyle(Konjo.ice)
                        .padding(8).background(Color.white.opacity(0.03), in: RoundedRectangle(cornerRadius: 6))
                    }

                    Text(cronHuman(cron)).font(Konjo.mono(10.5)).foregroundStyle(Konjo.fgMute)

                    let next = computeNextRuns(buildCronString(cron), from: Date())
                    if !next.isEmpty {
                        VStack(alignment: .leading, spacing: 2) {
                            ForEach(Array(next.enumerated()), id: \.offset) { _, date in
                                Text(date.formatted(date: .abbreviated, time: .shortened))
                                    .font(Konjo.mono(9.5)).foregroundStyle(Konjo.fgMute)
                            }
                        }
                    }
                }
            }
            .padding(14)
        }
    }

    private var timeRow: some View {
        Stepper("\(cron.hour12):\(String(format: "%02d", cron.min)) \(cron.ampm.rawValue)", value: Binding(
            get: { cron.hour12 }, set: { v in write { $0.cron.hour12 = ((v - 1 + 12) % 12) + 1 } }
        ), in: 1...12)
        .font(Konjo.mono(11)).foregroundStyle(Konjo.fgDim)
    }
}

// MARK: - Guardrails

private struct GuardrailsFacetView: View {
    let card: StackCard
    let write: (@escaping CardMutator) -> Void

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                guardLine(on: card.guardrails.gate, label: "gate", placeholder: "shell cmd, must pass first",
                          text: card.guardrails.gateCmd,
                          toggle: { write { $0.guardrails.gate.toggle() } },
                          setText: { v in write { $0.guardrails.gateCmd = v } })

                guardLine(on: card.guardrails.until, label: "until", placeholder: "loop until exit 0",
                          text: card.guardrails.untilCmd,
                          toggle: { write { $0.guardrails.until.toggle() } },
                          setText: { v in write { $0.guardrails.untilCmd = v } })

                labeledSegment("on fail", segmented([OnFail.stop, .continue, .backoff], selected: card.guardrails.onFail) { chosen in
                    write { $0.guardrails.onFail = chosen }
                })

                labeledSegment("budget", segmented([Budget.auto, .k200, .none], selected: card.guardrails.budget) { chosen in
                    write { $0.guardrails.budget = chosen }
                })

                HStack {
                    Text("max iter").font(Konjo.mono(10.5)).foregroundStyle(Konjo.fgDim)
                    Stepper("", value: Binding(
                        get: { card.maxIterations }, set: { v in write { $0.maxIterations = max(0, v) } }
                    ), in: 0...50).labelsHidden()
                    Text(cardIterationsLabel(card.maxIterations)).font(Konjo.mono(11)).foregroundStyle(Konjo.fg)
                    Spacer()
                }
            }
            .padding(14)
        }
    }

    private func labeledSegment(_ label: String, _ content: some View) -> some View {
        VStack(alignment: .leading, spacing: 5) {
            Text(label.uppercased()).font(Konjo.mono(8.5, weight: .bold)).tracking(1).foregroundStyle(Konjo.fgMute)
            content
        }
    }

    private func segmented<T: Hashable & RawRepresentable>(
        _ options: [T], selected: T, onSelect: @escaping (T) -> Void
    ) -> some View where T.RawValue == String {
        HStack(spacing: 6) {
            ForEach(options, id: \.self) { opt in
                Button(opt.rawValue) { onSelect(opt) }
                    .font(Konjo.mono(10))
                    .foregroundStyle(opt == selected ? Konjo.sun : Konjo.fgMute)
                    .padding(.horizontal, 8).padding(.vertical, 4)
                    .background(opt == selected ? Konjo.sun.opacity(0.16) : .clear, in: RoundedRectangle(cornerRadius: 5))
                    .overlay(RoundedRectangle(cornerRadius: 5).stroke(Konjo.line2, lineWidth: 1))
            }
        }
    }

    private func guardLine(on: Bool, label: String, placeholder: String, text: String,
                            toggle: @escaping () -> Void, setText: @escaping (String) -> Void) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 8) {
                Toggle("", isOn: Binding(get: { on }, set: { _ in toggle() })).labelsHidden().tint(Konjo.sun)
                Text(label).font(Konjo.mono(11.5)).foregroundStyle(Konjo.fg)
            }
            TextField(placeholder, text: Binding(get: { text }, set: setText))
                .font(Konjo.mono(10.5))
                .foregroundStyle(on ? Konjo.fg : Konjo.fgMute)
                .padding(8).background(Color.white.opacity(0.03), in: RoundedRectangle(cornerRadius: 6))
                .disabled(!on).opacity(on ? 1 : 0.4)
        }
    }
}

// `EvalsFacetView`, `GoalFacetView`, `MaxxFacetView`, and `ConfigFacetView`
// live in `CardFacetViews.swift` — kept out of this file to stay under the
// repo's file-size gate.
