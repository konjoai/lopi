import SwiftUI
import LopiStacksKit

/// "Stack Loops" overview — every stack pane grouped by lifecycle phase, the
/// iOS mobile counterpart to `web/src/routes/stacks` (production layout; see
/// the design handoff's `OverviewProduction`): swipe-to-manage on each card,
/// and a "+ New Stack" FAB that opens a blank pane full-screen. Tapping a
/// card pushes into that pane's `StackDetailScreen`.
struct StackOverviewScreen: View {
    @Environment(AppModel.self) private var model
    @State private var configOpen = false
    @State private var path = NavigationPath()
    @State private var showNewStack = false
    @State private var newStackKey: String?

    private var panes: [StackPaneState] { model.stackStore.panes }

    private var totalCost: Double {
        model.activeAgents.reduce(0) { $0 + $1.costUsd }
    }

    private var runningCount: Int {
        panes.filter { StackDisplay.overviewPhase(for: $0, liveAgents: model.liveAgents).isActiveBucket }.count
    }

    var body: some View {
        NavigationStack(path: $path) {
            ZStack(alignment: .bottomTrailing) {
                list
                NewStackFAB(action: startNewStack)
                    .padding(.trailing, 18)
                    .padding(.bottom, 26)
            }
            .background(Konjo.deep)
            .navigationTitle("")
            .toolbar(.hidden, for: .navigationBar)
            .safeAreaInset(edge: .top) { header }
            .navigationDestination(for: String.self) { key in
                if let pane = model.stackStore.pane(for: key) {
                    StackDetailScreen(paneKey: pane.key)
                }
            }
            .sheet(isPresented: $configOpen) { ServerConfigScreen() }
            .fullScreenCover(isPresented: $showNewStack, onDismiss: cleanupNewStackIfEmpty) {
                if let key = newStackKey { StackDetailScreen(paneKey: key) }
            }
        }
    }

    private var list: some View {
        List {
            ForEach(OverviewPhase.allCases) { phase in
                let items = panes.filter {
                    StackDisplay.overviewPhase(for: $0, liveAgents: model.liveAgents) == phase
                }
                if !items.isEmpty {
                    Section {
                        ForEach(items) { pane in
                            StackOverviewCard(pane: pane)
                                .contentShape(Rectangle())
                                .onTapGesture { path.append(pane.key) }
                                .listRowInsets(EdgeInsets(top: 5, leading: 18, bottom: 5, trailing: 18))
                                .listRowBackground(Konjo.deep)
                                .listRowSeparator(.hidden)
                                .swipeActions(edge: .trailing, allowsFullSwipe: true) {
                                    Button(role: .destructive) { deleteStack(pane.key) } label: {
                                        Label("Delete", systemImage: "trash.fill")
                                    }
                                    .tint(Konjo.rose)
                                    pauseResumeButton(for: pane)
                                }
                        }
                    } header: {
                        phaseSectionHeader(phase, count: items.count)
                    }
                }
            }
        }
        .listStyle(.plain)
        .scrollContentBackground(.hidden)
        .contentMargins(.bottom, 90, for: .scrollContent)
    }

    private func pauseResumeButton(for pane: StackPaneState) -> some View {
        let paused = model.stackEngine.run(for: pane.key)?.phase == .paused
        return Button {
            if paused {
                model.stackEngine.resumeStack(pane.key, PaneDefaults(pane.config.defaults))
            } else {
                model.stackEngine.pauseStack(pane.key)
            }
        } label: {
            Label(paused ? "Resume" : "Pause", systemImage: paused ? "play.fill" : "pause.fill")
        }
        .tint(Color(hex: 0x3A3A3D))
    }

    private func deleteStack(_ key: String) {
        model.stackEngine.clearRun(key)
        model.stackStore.deleteStackFromPanes(key)
    }

    /// Creates the blank pane immediately (mirrors macOS `ForgeView`'s
    /// always-materialized empty pane) and opens it full-screen.
    private func startNewStack() {
        model.stackStore.addStackPane()
        newStackKey = model.stackStore.panes.last?.key
        showNewStack = true
    }

    /// Tears the pane back down if the user backed out without committing a
    /// card, so cancelling never litters the Overview with empty stacks.
    private func cleanupNewStackIfEmpty() {
        defer { newStackKey = nil }
        guard let key = newStackKey else { return }
        if model.stackStore.pane(for: key)?.cards.isEmpty ?? true {
            model.stackEngine.clearRun(key)
            model.stackStore.deleteStackFromPanes(key)
        }
    }

    private var connectionLabel: String {
        switch model.connection {
        case .live: return "LIVE"
        case .connecting: return "CONNECTING"
        case .offline: return "OFFLINE"
        }
    }

    private var connectionColor: Color {
        switch model.connection {
        case .live: return Konjo.jade
        case .connecting: return Konjo.sun
        case .offline: return Konjo.fgMute
        }
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack(spacing: 8) {
                Text("Stack Loops")
                    .font(Konjo.sans(22, weight: .heavy))
                    .foregroundStyle(Konjo.fg)
                Spacer()
                Button { configOpen = true } label: {
                    Image(systemName: "gearshape").font(.system(size: 15)).foregroundStyle(Konjo.fgMute)
                }
                .buttonStyle(.plain)
                HStack(spacing: 5) {
                    Circle().fill(connectionColor).frame(width: 6, height: 6)
                    Text(connectionLabel).font(Konjo.mono(10)).foregroundStyle(connectionColor)
                }
            }
            Text("\(panes.count) stacks · \(runningCount) running · $\(String(format: "%.4f", totalCost)) today")
                .font(Konjo.mono(10.5))
                .foregroundStyle(Konjo.fgMute)
            Text("swipe a card left to pause or delete")
                .font(Konjo.mono(9.5))
                .foregroundStyle(Konjo.fgMute)
        }
        .padding(.horizontal, 18)
        .padding(.top, 8)
        .padding(.bottom, 10)
        .background(Konjo.deep)
    }

    private func phaseSectionHeader(_ phase: OverviewPhase, count: Int) -> some View {
        HStack(spacing: 8) {
            Circle().fill(phase.color).frame(width: 6, height: 6)
            Text(phase.label)
                .font(Konjo.mono(11, weight: .bold))
                .tracking(0.6)
                .foregroundStyle(phase.color)
            Spacer()
            Text("\(count)").font(Konjo.mono(10.5)).foregroundStyle(Konjo.fgMute)
        }
        .padding(.horizontal, 18)
        .padding(.vertical, 8)
        .background(.ultraThinMaterial.opacity(0.001)) // forces the material stacking context for the blur below
        .background(Konjo.deep.opacity(0.92))
        .background(.ultraThinMaterial)
        .overlay(alignment: .top) { Rectangle().fill(Konjo.line).frame(height: 1) }
        .overlay(alignment: .bottom) { Rectangle().fill(Konjo.line).frame(height: 1) }
    }
}

/// One stack's summary card — reused by the Overview list. Left border +
/// status dot in the pane's phase accent; loop-count badge; a representative
/// prompt line; a per-loop mini progress bar; a repo/branch + elapsed-cost
/// (or state word) meta line.
private struct StackOverviewCard: View {
    @Environment(AppModel.self) private var model
    let pane: StackPaneState

    private var phase: OverviewPhase {
        StackDisplay.overviewPhase(for: pane, liveAgents: model.liveAgents)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 9) {
            HStack(spacing: 7) {
                PulsingDot(color: phase.color, pulsing: phase == .running || phase == .testing)
                Text(pane.title)
                    .font(Konjo.sans(14.5, weight: .bold))
                    .foregroundStyle(Konjo.fg)
                Spacer()
                Text("×\(pane.cards.count)")
                    .font(Konjo.mono(10))
                    .foregroundStyle(Konjo.fgMute)
                    .padding(.horizontal, 6).padding(.vertical, 1)
                    .overlay(RoundedRectangle(cornerRadius: 5).stroke(Konjo.line2, lineWidth: 1))
            }

            Text(StackDisplay.representativeGoal(for: pane, liveAgents: model.liveAgents))
                .font(Konjo.sans(12.5))
                .foregroundStyle(Konjo.fgDim)
                .lineLimit(2)

            miniBar

            HStack {
                Text(repoLine).font(Konjo.mono(10)).foregroundStyle(Konjo.fgMute)
                Spacer()
                Text(metaLine).font(Konjo.mono(10)).foregroundStyle(metaColor)
            }
        }
        .padding(13)
        .background(Konjo.bg1)
        .clipShape(.rect(topLeadingRadius: 0, bottomLeadingRadius: 10, bottomTrailingRadius: 10, topTrailingRadius: 10))
        .overlay(
            UnevenRoundedRectangle(
                topLeadingRadius: 0, bottomLeadingRadius: 10, bottomTrailingRadius: 10, topTrailingRadius: 10
            )
            .stroke(phase == .running || phase == .testing ? phase.color.opacity(0.35) : Konjo.line, lineWidth: 1)
        )
        .overlay(alignment: .leading) {
            Rectangle().fill(phase.color).frame(width: 3)
        }
    }

    private var miniBar: some View {
        HStack(spacing: 3) {
            ForEach(pane.cards) { card in
                let status = StackDisplay.cardStatus(card, liveAgents: model.liveAgents)
                RoundedRectangle(cornerRadius: 2)
                    .fill(status == .queued ? Konjo.fgMute.opacity(0.3) : status.color)
                    .frame(height: 4)
            }
        }
    }

    private var repoLine: String {
        let repo = pane.config.defaults.repo.isEmpty ? "no repo set" : pane.config.defaults.repo
        return pane.config.defaults.branch.isEmpty ? repo : "\(repo) · \(pane.config.defaults.branch)"
    }

    private var metaLine: String {
        if let live = StackDisplay.elapsedAndCost(for: pane, liveAgents: model.liveAgents) { return live }
        if phase == .done, pane.cards.contains(where: {
            if case .blocked = StackDisplay.cardStatus($0, liveAgents: model.liveAgents) { return true }
            return false
        }) { return "failed" }
        if pane.cards.isEmpty { return "queued" }
        return phase.label.lowercased()
    }

    private var metaColor: Color {
        metaLine == "failed" ? Konjo.rose : phase.color
    }
}

/// A status dot with an optional gentle pulse animation, matching the
/// design's `iospulse` keyframe (opacity 1 ↔ 0.4).
struct PulsingDot: View {
    let color: Color
    var pulsing: Bool = false
    @State private var dim = false

    var body: some View {
        Circle()
            .fill(color)
            .frame(width: 7, height: 7)
            .opacity(pulsing ? (dim ? 0.4 : 1) : 1)
            .onAppear {
                guard pulsing else { return }
                withAnimation(.easeInOut(duration: 0.9).repeatForever(autoreverses: true)) {
                    dim = true
                }
            }
    }
}

private extension OverviewPhase {
    var isActiveBucket: Bool { self == .running || self == .testing }
}
