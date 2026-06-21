import SwiftUI

/// One pane in the Forge grid, laid out to mirror the web UI's AgentPane: a
/// content column (header · orb · metrics · log strip · command · footer) beside
/// a narrow right rail (close · phase · retry/stop). An empty pane becomes a
/// launcher; a live pane shows cognition and exposes its controls on the rail.
struct AgentPaneView: View {
    @Environment(AppModel.self) private var model
    var agent: LiveAgent?
    var controls: LaunchControls
    /// Total panes in the grid — the orb scales down as more are added.
    var paneCount: Int = 1
    var onClose: () -> Void

    @State private var goal = ""
    @State private var submitting = false
    @State private var deciding = false

    private var accent: Color { agent.map { PhaseStyle.color($0.phase) } ?? Konjo.konjo }
    private var isLive: Bool { agent.map { PhaseStyle.isActive($0.phase) && $0.active } ?? false }

    /// Rail phase label — a clean "Review" while gated, else the phase name.
    private var railPhaseLabel: String {
        guard let agent else { return "—" }
        return agent.awaitingApproval ? "Review" : agent.phase.capitalized
    }

    /// Orb diameter scaled to the pane's share of the screen: large in a
    /// single pane, shrinking smoothly (∝ √areaFraction) as panes are added so
    /// it always sits comfortably within its tile.
    private var orbSize: CGFloat {
        let (cols, rows) = PaneLayout.dims(max(paneCount, 1))
        let frac = (1.0 / Double(cols * rows)).squareRoot()
        return min(462, max(159, (40 + 220 * frac) * 1.65))
    }

    /// Pane text runs 25% larger than the base Konjo type scale for legibility.
    private static let textScale: CGFloat = 1.25
    private func paneMono(_ size: CGFloat, weight: Font.Weight = .regular) -> Font {
        Konjo.mono(size * Self.textScale, weight: weight)
    }
    private func paneSans(_ size: CGFloat, weight: Font.Weight = .regular) -> Font {
        Konjo.sans(size * Self.textScale, weight: weight)
    }

    var body: some View {
        HStack(spacing: 0) {
            contentColumn
            rightRail
        }
        .background(Konjo.bg1.opacity(0.6))
        .clipShape(RoundedRectangle(cornerRadius: 10))
        // Resting hairline, plus a phase-tinted rim while the agent is working
        // so a busy grid telegraphs which panes are live at a glance.
        .overlay(
            RoundedRectangle(cornerRadius: 10)
                .stroke(isLive ? accent.opacity(0.35) : Konjo.line, lineWidth: 1)
        )
        // Floating-card elevation; the rim glow intensifies on live panes.
        .shadow(color: .black.opacity(0.55), radius: 14, y: 6)
        .shadow(color: isLive ? accent.opacity(0.28) : .clear, radius: 18)
        .animation(.easeInOut(duration: 0.4), value: isLive)
    }

    // MARK: Content column (left)

    private var contentColumn: some View {
        VStack(spacing: 0) {
            header
            Divider().overlay(Konjo.line)
            orbArea
            if let agent {
                metrics(agent)
                logStrip(agent)
            }
            commandBar
            if let agent { footer(agent) }
        }
        .frame(maxWidth: .infinity)
    }

    private var header: some View {
        HStack(spacing: 8) {
            Circle().fill(accent).frame(width: 7, height: 7)
                .shadow(color: isLive ? accent.opacity(0.9) : .clear, radius: 5)
            Text(agent?.goal ?? "— idle —")
                .font(paneMono(11, weight: .medium)).lineLimit(1)
                .foregroundStyle(agent == nil ? Konjo.fgMute : Konjo.fg)
            Spacer(minLength: 8)
            // Phase reads right-aligned on the title line.
            if agent != nil {
                Text(railPhaseLabel)
                    .font(paneSans(11, weight: .bold))
                    .foregroundStyle(agent?.awaitingApproval == true ? Konjo.sun : accent)
                    .lineLimit(1).fixedSize()
            }
        }
        .padding(.horizontal, 12).padding(.vertical, 9)
    }

    /// The flexible middle — orb (+ aura) that pushes the fixed strips to the
    /// bottom. An empty pane shows the launcher selectors beneath the orb.
    private var orbArea: some View {
        VStack(spacing: 16) {
            KonjoOrb(
                phase: agent?.phase ?? "idle",
                activity: agent?.activity ?? 0,
                pressure: agent?.pressure ?? 0,
                health: agent?.testPassRate ?? 0.85,
                stimulus: agent?.stimulus ?? .distantPast,
                stimulusKind: agent?.stimulusKind ?? "request",
                size: orbSize
            )
            .background(
                Circle()
                    .fill(accent.opacity(agent == nil ? 0.05 : 0.16))
                    .frame(width: orbSize * 1.24, height: orbSize * 1.24)
                    .blur(radius: 26)
            )
            // Grow/shrink in step with the grid's add/remove spring.
            .animation(.spring(response: 0.42, dampingFraction: 0.82), value: paneCount)
            if agent == nil {
                LaunchControlsView(controls: controls, dense: true)
                    .padding(.horizontal, 14)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .padding(.vertical, 16)
        // Phase 11 — the plan approval gate takes over the orb area while paused.
        .overlay {
            if let agent, agent.awaitingApproval { planGate(agent) }
        }
    }

    // MARK: Plan approval gate (Phase 11)

    private func planGate(_ agent: LiveAgent) -> some View {
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                Image(systemName: "pause.circle.fill").foregroundStyle(Konjo.sun)
                Text("Plan ready · review").font(paneSans(12, weight: .bold)).foregroundStyle(Konjo.sun)
                Spacer()
                Text("attempt \(agent.attempt)").font(paneMono(8)).foregroundStyle(Konjo.fgMute)
            }
            .padding(.horizontal, 12).padding(.vertical, 9)
            .overlay(Rectangle().fill(Konjo.line).frame(height: 1), alignment: .bottom)

            ScrollView {
                VStack(alignment: .leading, spacing: 6) {
                    if agent.planSteps.isEmpty {
                        Text(agent.planText.isEmpty ? "—" : agent.planText)
                            .font(paneMono(10)).foregroundStyle(Konjo.fgDim)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    } else {
                        ForEach(Array(agent.planSteps.enumerated()), id: \.offset) { i, step in
                            HStack(alignment: .top, spacing: 8) {
                                Text("\(i + 1).").font(paneMono(10)).foregroundStyle(Konjo.sun.opacity(0.7))
                                Text(step).font(paneMono(10)).foregroundStyle(Konjo.fgDim)
                                Spacer(minLength: 0)
                            }
                        }
                    }
                }
                .padding(12)
            }

            HStack(spacing: 10) {
                Button { decide(agent, approve: true) } label: {
                    Text("✓ Approve").frame(maxWidth: .infinity)
                }
                .konjoButton(Konjo.jade)
                Button { decide(agent, approve: false) } label: {
                    Text("✕ Reject").frame(maxWidth: .infinity)
                }
                .konjoButton(Konjo.rose)
            }
            .disabled(deciding)
            .padding(12)
            .overlay(Rectangle().fill(Konjo.line).frame(height: 1), alignment: .top)
        }
        .background(Konjo.deep.opacity(0.96))
        .clipShape(RoundedRectangle(cornerRadius: 10))
        .overlay(RoundedRectangle(cornerRadius: 10).stroke(Konjo.sun.opacity(0.4), lineWidth: 1))
        .padding(10)
    }

    private func decide(_ agent: LiveAgent, approve: Bool) {
        guard !deciding else { return }
        deciding = true
        Task {
            await model.decidePlan(agent.id, approve: approve)
            await MainActor.run { deciding = false }
        }
    }

    // MARK: Metrics strip

    private func metrics(_ agent: LiveAgent) -> some View {
        HStack(spacing: 12) {
            meter("P", value: agent.pressure, warn: agent.pressure > 0.75)
            label("A", "\(Int(agent.activity * 100))")
            label("$", String(format: "%.4f", agent.costUsd))
        }
        .padding(.horizontal, 12).padding(.vertical, 7)
        .background(Color.black.opacity(0.2))
    }

    private func meter(_ k: String, value: Double, warn: Bool) -> some View {
        HStack(spacing: 5) {
            Text("\(k):").font(paneMono(9)).foregroundStyle(Konjo.fgMute)
            GeometryReader { g in
                ZStack(alignment: .leading) {
                    Capsule().fill(Color.black.opacity(0.4))
                    Capsule().fill(warn ? Konjo.rose : Konjo.konjo2)
                        .frame(width: g.size.width * CGFloat(min(max(value, 0), 1)))
                }
            }
            .frame(height: 4)
        }
        .frame(maxWidth: .infinity)
    }

    private func label(_ k: String, _ v: String) -> some View {
        HStack(spacing: 4) {
            Text("\(k):").font(paneMono(9)).foregroundStyle(Konjo.fgMute)
            Text(v).font(paneMono(9)).foregroundStyle(Konjo.fgDim).monospacedDigit()
        }
    }

    // MARK: Log strip — last few lines for this agent

    private func logStrip(_ agent: LiveAgent) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            if agent.logTail.isEmpty {
                Text("— waiting for output —")
                    .font(paneMono(8)).italic().foregroundStyle(Konjo.fgMute)
            } else {
                ForEach(Array(agent.logTail.suffix(3).enumerated()), id: \.offset) { _, log in
                    HStack(spacing: 6) {
                        Text("[\(log.level.prefix(1).uppercased())]")
                            .foregroundStyle(logColor(log.level))
                        Text(log.text).lineLimit(1).foregroundStyle(Konjo.fgDim)
                    }
                    .font(paneMono(8))
                }
            }
        }
        .frame(maxWidth: .infinity, minHeight: 30, alignment: .leading)
        .padding(.horizontal, 12).padding(.vertical, 6)
        .background(Color.black.opacity(0.3))
    }

    private func logColor(_ level: String) -> Color {
        switch level {
        case "error": return Konjo.rose
        case "warn": return Konjo.flame
        default: return Konjo.fgMute
        }
    }

    // MARK: Command bar

    private var commandBar: some View {
        HStack(spacing: 8) {
            Text(">").font(paneMono(16, weight: .medium)).foregroundStyle(Konjo.ok)
            TextField(agent == nil ? "type a goal…" : "new goal…", text: $goal)
                .textFieldStyle(.plain)
                .font(paneMono(16)).foregroundStyle(Konjo.fg)
                .onSubmit { submit(goal: goal) }
            if submitting { ProgressView().controlSize(.small) }
        }
        .padding(.horizontal, 14).padding(.top, 12).padding(.bottom, 18)
        .background(Color.black.opacity(0.1))
    }

    // MARK: Footer — attempt · branch

    private func footer(_ agent: LiveAgent) -> some View {
        HStack {
            Text("attempt \(agent.attempt)")
            Spacer()
            if let branch = agent.branch { Text(branch).lineLimit(1) }
        }
        .font(paneMono(8)).foregroundStyle(Konjo.fgMute)
        .padding(.horizontal, 12).padding(.vertical, 5)
    }

    // MARK: Right rail — a slim control strip (close · retry/stop). The phase
    // now reads on the title line.

    private var rightRail: some View {
        VStack(spacing: 0) {
            Button(action: onClose) {
                Image(systemName: "xmark").font(.system(size: 9, weight: .bold))
                    .foregroundStyle(Konjo.fgDim)
                    .frame(width: 20, height: 20)
                    .background(Color.white.opacity(0.08))
                    .clipShape(Circle())
            }
            .buttonStyle(.plain)
            .help(agent == nil ? "Close pane" : "Close pane (session stays in sidebar)")

            Spacer(minLength: 8)

            if let agent {
                VStack(spacing: 10) {
                    railButton("arrow.clockwise", Konjo.sun, help: "Retry task") {
                        submit(goal: agent.goal)
                    }
                    railButton("stop.fill", Konjo.rose, disabled: !agent.active, help: "Stop / cancel") {
                        Task { await model.cancelTask(agent.id) }
                    }
                }
            }
        }
        .frame(width: 56)
        .padding(.vertical, 12)
        .padding(.horizontal, 6)
        .background(Color.black.opacity(0.3))
        .overlay(Rectangle().fill(Konjo.line).frame(width: 1), alignment: .leading)
    }

    /// A 44pt square rail control matching the web's retry/stop buttons.
    private func railButton(_ icon: String, _ color: Color, disabled: Bool = false,
                            help: String, _ action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: icon)
                .font(.system(size: 15))
                .foregroundStyle(color)
                .frame(width: 44, height: 44)
                .background(RoundedRectangle(cornerRadius: 8).fill(color.opacity(0.06)))
                .overlay(RoundedRectangle(cornerRadius: 8).stroke(Konjo.line2, lineWidth: 1))
        }
        .buttonStyle(KonjoIconButtonStyle())
        .disabled(disabled)
        .opacity(disabled ? 0.3 : 1)
        .help(help)
    }

    // MARK: Submit

    private func submit(goal text: String) {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty, !submitting else { return }
        submitting = true
        Task {
            await model.submitTask(controls.body(goal: trimmed))
            await MainActor.run {
                goal = ""
                submitting = false
            }
        }
    }
}

/// Plain icon button that just dips on press (no chrome of its own).
private struct KonjoIconButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .scaleEffect(configuration.isPressed ? 0.92 : 1)
            .animation(.easeOut(duration: 0.12), value: configuration.isPressed)
    }
}
