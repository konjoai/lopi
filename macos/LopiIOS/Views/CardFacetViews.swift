import SwiftUI
import LopiStacksKit

/// `evals`/`goal`/`maxx`/`config` facet tab content — split out of
/// `FacetPopovers.swift` (which holds the tab wrapper plus `schedule`/
/// `guardrails`) purely to stay under the repo's per-file line-count gate.

// MARK: - Evals

struct EvalsFacetView: View {
    let card: StackCard
    let write: (@escaping CardMutator) -> Void

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 4) {
                ForEach(EVAL_CATALOG, id: \.name) { ref in
                    let isBaseline = ref.tier == .base
                    let on = card.evals.contains(ref)
                    Button {
                        write { c in
                            if let idx = c.evals.firstIndex(of: ref) { c.evals.remove(at: idx) }
                            else { c.evals.append(ref) }
                        }
                    } label: {
                        HStack(spacing: 8) {
                            Image(systemName: on ? "checkmark.square.fill" : "square")
                                .foregroundStyle(on ? Konjo.jade : Konjo.fgMute)
                            Text(ref.name).font(Konjo.mono(11.5)).foregroundStyle(Konjo.fg)
                            Spacer()
                            Text(ref.tier.rawValue.uppercased())
                                .font(Konjo.mono(8, weight: .bold))
                                .foregroundStyle(tierColor(ref.tier))
                                .padding(.horizontal, 5).padding(.vertical, 2)
                                .overlay(RoundedRectangle(cornerRadius: 4).stroke(tierColor(ref.tier).opacity(0.5), lineWidth: 1))
                        }
                        .padding(.vertical, 6)
                        .opacity(isBaseline ? 0.6 : 1)
                    }
                    .buttonStyle(.plain)
                    .disabled(isBaseline)
                }

                Divider().overlay(Konjo.line).padding(.vertical, 4)

                Text("SUITE").font(Konjo.mono(8.5, weight: .bold)).tracking(1).foregroundStyle(Konjo.fgMute)
                HStack(spacing: 6) {
                    ForEach(EVAL_SUITES.keys.sorted(), id: \.self) { key in
                        Button(key) {
                            write { c in
                                for name in EVAL_SUITES[key] ?? [] {
                                    guard let ref = EVAL_CATALOG.first(where: { $0.name == name }),
                                          !c.evals.contains(ref)
                                    else { continue }
                                    c.evals.append(ref)
                                }
                            }
                        }
                        .font(Konjo.mono(10))
                        .foregroundStyle(key == "kcqf" ? Konjo.sun : Konjo.fgDim)
                        .padding(.horizontal, 8).padding(.vertical, 4)
                        .overlay(
                            RoundedRectangle(cornerRadius: 5)
                                .strokeBorder(style: StrokeStyle(lineWidth: 1, dash: [3, 3]))
                                .foregroundStyle(key == "kcqf" ? Konjo.sun.opacity(0.5) : Konjo.line2)
                        )
                    }
                }
            }
            .padding(14)
        }
    }

    private func tierColor(_ tier: EvalTier) -> Color {
        switch tier {
        case .base: return Konjo.jade
        case .test: return Konjo.ice
        case .judge: return Konjo.stackViolet
        case .suite: return Konjo.sun
        }
    }
}

// MARK: - Goal (pane/stack scope — shared across every card, matching web)

struct GoalFacetView: View {
    @Environment(AppModel.self) private var model
    let paneKey: String

    private var goal: StackGoal { model.stackStore.pane(for: paneKey)?.config.goal ?? defaultStackGoal() }

    private func write(_ mutate: @escaping (inout StackConfig) -> Void) {
        model.stackStore.updateStackConfig(paneKey, mutate)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("re-runs the whole chain of loops until the chain-acceptance evals pass — \u{201C}pursue goal\u{201D} instead of a single \u{201C}run stack.\u{201D}")
                .font(Konjo.sans(11)).foregroundStyle(Konjo.fgMute)

            HStack(spacing: 8) {
                Toggle("", isOn: Binding(
                    get: { goal.pursue }, set: { v in write { $0.goal.pursue = v } }
                )).labelsHidden().tint(Konjo.flame)
                Text("pursue").font(Konjo.mono(11.5)).foregroundStyle(Konjo.fg)
            }

            HStack {
                Text("no-progress limit").font(Konjo.mono(10.5)).foregroundStyle(Konjo.fgDim)
                Spacer()
                Stepper("", value: Binding(
                    get: { goal.noProgressLimit }, set: { v in write { $0.goal.noProgressLimit = max(0, v) } }
                ), in: 0...20).labelsHidden()
                Text(goal.noProgressLimit == 0 ? "off" : "\(goal.noProgressLimit)")
                    .font(Konjo.mono(11)).foregroundStyle(Konjo.fg)
            }

            Text("stop after this many consecutive chain-runs with no gain; 0 disables the check.")
                .font(Konjo.sans(10)).foregroundStyle(Konjo.fgMute)
        }
        .padding(14)
    }
}

// MARK: - MAXX

struct MaxxFacetView: View {
    @Environment(AppModel.self) private var model
    let card: StackCard
    let write: (@escaping CardMutator) -> Void

    @State private var busy = false
    @State private var error: String?
    @State private var quota: QuotaSnapshot?

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                HStack(spacing: 8) {
                    Toggle("", isOn: Binding(get: { card.maxx.enabled }, set: { _ in Task { await toggle() } }))
                        .labelsHidden().tint(Konjo.flame).disabled(busy)
                    Text("enable MAXX").font(Konjo.mono(11.5)).foregroundStyle(Konjo.fg)
                }
                if let error {
                    Text(error).font(Konjo.mono(9.5)).foregroundStyle(Konjo.rose)
                }

                Text("RUN").font(Konjo.mono(8.5, weight: .bold)).tracking(1).foregroundStyle(Konjo.fgMute)
                VStack(alignment: .leading, spacing: 3) {
                    Text("• after hours \(quietHoursLabel)")
                    Text("• nearing quota reset with high headroom")
                }
                .font(Konjo.mono(10)).foregroundStyle(Konjo.fgDim)

                Text("CURRENT QUOTA").font(Konjo.mono(8.5, weight: .bold)).tracking(1).foregroundStyle(Konjo.fgMute)
                quotaBar(label: "5h window", window: quota?.fiveHour, color: Konjo.ice)
                quotaBar(label: "7d window", window: quota?.sevenDay, color: Konjo.jade)
            }
            .padding(14)
        }
        .task { await loadQuota() }
    }

    private var quietHoursLabel: String {
        let hours = card.maxx.quietHours
        guard hours.count >= 2 else { return "—" }
        return "\(fmtHour12(hours[0]))–\(fmtHour12(hours[1]))"
    }

    private func toggle() async {
        guard !busy else { return }
        busy = true; error = nil
        defer { busy = false }
        let next = !card.maxx.enabled
        do {
            var entryId = card.maxxEntryId
            if next {
                if let id = entryId {
                    try await model.client.enableMaxx(id: id)
                } else {
                    let created = try await model.client.createMaxx(MaxxBody(
                        name: card.goal.isEmpty ? "maxx entry" : String(card.goal.prefix(60)),
                        goal: card.goal, repo: card.config.repo, priority: nil, enabled: true,
                        autonomyLevel: nil, report: nil,
                        quietHours: card.maxx.quietHours, headroomGate: card.maxx.headroomGate,
                        windows: card.maxx.windows.map(\.rawValue)
                    ))
                    entryId = created.id
                }
            } else if let id = entryId {
                try await model.client.disableMaxx(id: id)
            }
            let finalId = entryId
            write { $0.maxx.enabled = next; $0.maxxEntryId = finalId }
        } catch {
            self.error = error.localizedDescription
        }
    }

    private func loadQuota() async {
        quota = try? await model.client.quota()
    }

    private func fmtHour12(_ h: Int) -> String {
        let hour = h % 24
        let period = hour < 12 ? "AM" : "PM"
        let display = hour % 12 == 0 ? 12 : hour % 12
        return "\(display)\(period)"
    }

    private func quotaBar(label: String, window: QuotaWindow?, color: Color) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            HStack {
                Text(label).font(Konjo.mono(10)).foregroundStyle(Konjo.fgDim)
                Spacer()
                Text(window.map { "\(Int($0.utilization * 100))%" } ?? "no data yet")
                    .font(Konjo.mono(10)).foregroundStyle(Konjo.fgMute)
            }
            RoundedRectangle(cornerRadius: 2)
                .fill(Color.white.opacity(0.06))
                .frame(height: 5)
                .overlay(alignment: .leading) {
                    GeometryReader { geo in
                        RoundedRectangle(cornerRadius: 2)
                            .fill(color)
                            .frame(width: geo.size.width * CGFloat(window?.utilization ?? 0))
                    }
                }
        }
    }
}

// MARK: - Config (card-scoped, falls back to the pane's `StackDefaults`)

struct ConfigFacetView: View {
    @Environment(AppModel.self) private var model
    let paneKey: String
    let card: StackCard
    let write: (@escaping CardMutator) -> Void

    private var defaults: StackDefaults {
        model.stackStore.pane(for: paneKey)?.config.defaults
            ?? StackDefaults(model: "", effort: "", repo: "", branch: "", autonomy: "")
    }

    private var effectiveRepo: String { card.config.repo ?? defaults.repo }

    private var branchOptions: [StackOption] {
        (model.branchesByRepo[effectiveRepo] ?? []).map { StackOption(value: $0, label: $0) }
    }

    private var resolvedBranch: String {
        resolveBranch(card.config.branch ?? defaults.branch, model.branchesByRepo[effectiveRepo] ?? [],
                      model.headBranchByRepo[effectiveRepo] ?? "")
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            configRow("model", card.config.model ?? defaults.model, MODEL_OPTIONS) { v in write { $0.config.model = v } }
            configRow("effort", card.config.effort ?? defaults.effort, EFFORT_OPTIONS) { v in write { $0.config.effort = v } }
            configRow("repo", effectiveRepo, repoOptions(model.repos)) { v in write { $0.config.repo = v } }
            configRow("branch", resolvedBranch, branchOptions) { v in write { $0.config.branch = v } }
            configRow("autonomy", card.config.autonomy ?? defaults.autonomy, AUTONOMY_OPTIONS) { v in write { $0.config.autonomy = v } }
        }
        .padding(14)
        .task(id: effectiveRepo) { await model.ensureBranches(effectiveRepo) }
    }

    private func configRow(_ label: String, _ value: String, _ options: [StackOption], onSelect: @escaping (String) -> Void) -> some View {
        HStack {
            Text(label).font(Konjo.mono(10.5)).foregroundStyle(Konjo.fgDim)
            Spacer()
            Menu {
                ForEach(options, id: \.value) { opt in
                    Button(opt.label) { onSelect(opt.value) }
                }
            } label: {
                HStack(spacing: 4) {
                    Text(value.isEmpty ? "—" : value).font(Konjo.mono(11)).foregroundStyle(Konjo.fg)
                    Image(systemName: "chevron.down").font(.system(size: 8)).foregroundStyle(Konjo.fgMute)
                }
            }
        }
    }
}
