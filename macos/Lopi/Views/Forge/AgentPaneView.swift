import SwiftUI

/// One pane in the Forge grid: an orb cockpit on the left, and a live Claude
/// output log that slides in from the right (1/3 width) when a goal is
/// submitted. An empty pane is a launcher; a live pane shows cognition, the real
/// streamed Claude status, and exposes its controls inline.
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
    /// Whether the log panel is visible. Becomes true the moment a goal is submitted.
    @State private var showLog = false

    private var accent: Color { agent.map { PhaseStyle.color($0.phase) } ?? Konjo.konjo }
    private var isLive: Bool { agent.map { PhaseStyle.isActive($0.phase) && $0.active } ?? false }

    /// Phase label — a clean "Review" while gated, else the phase name.
    private var phaseLabel: String {
        guard let agent else { return "—" }
        return agent.awaitingApproval ? "Review" : agent.phase.capitalized
    }

    /// Orb diameter scaled to the pane's share of the screen: large in a
    /// single pane, shrinking smoothly (∝ √areaFraction) as panes are added.
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
        GeometryReader { geo in
            HStack(spacing: 0) {
                cockpit
                    .frame(width: showLog && agent != nil
                        ? geo.size.width * 2.0 / 3.0
                        : geo.size.width)

                if showLog, let agent {
                    Rectangle().fill(Konjo.line).frame(width: 1)
                    logPanel(agent)
                        .frame(width: geo.size.width / 3.0)
                        .transition(.move(edge: .trailing).combined(with: .opacity))
                }
            }
            .animation(.spring(response: 0.42, dampingFraction: 0.82), value: showLog)
        }
        .background(Konjo.bg1.opacity(0.6))
        .clipShape(RoundedRectangle(cornerRadius: 10))
        // Resting hairline, plus a phase-tinted rim while the agent is working.
        .overlay(
            RoundedRectangle(cornerRadius: 10)
                .stroke(isLive ? accent.opacity(0.35) : Konjo.line, lineWidth: 1)
        )
        .shadow(color: .black.opacity(0.55), radius: 14, y: 6)
        .shadow(color: isLive ? accent.opacity(0.28) : .clear, radius: 18)
        .animation(.easeInOut(duration: 0.4), value: isLive)
    }

    // MARK: Left column — orb cockpit

    private var cockpit: some View {
        VStack(spacing: 0) {
            header
            Divider().overlay(Konjo.line)
            orbArea
            // Mini log strip is only shown when the full log panel is closed.
            if let agent, !showLog {
                logStrip(agent)
            }
            Divider().overlay(Konjo.line)
            if let agent { metrics(agent) }
            commandBar
            if let agent { bottomBar(agent) }
        }
        .frame(maxWidth: .infinity)
    }

    // MARK: Header

    private var header: some View {
        HStack(spacing: 8) {
            Circle().fill(accent).frame(width: 7, height: 7)
                .shadow(color: isLive ? accent.opacity(0.9) : .clear, radius: 5)
            Text(agent?.goal ?? "— idle —")
                .font(paneMono(11, weight: .medium)).lineLimit(1)
                .foregroundStyle(agent == nil ? Konjo.fgMute : Konjo.fg)
            Spacer(minLength: 8)
            if agent != nil {
                Text(phaseLabel)
                    .font(paneSans(11, weight: .bold))
                    .foregroundStyle(agent?.awaitingApproval == true ? Konjo.sun : accent)
                    .lineLimit(1).fixedSize()
            }
            Button(action: onClose) {
                Image(systemName: "xmark").font(.system(size: 10, weight: .bold))
                    .foregroundStyle(Konjo.fgDim)
                    .frame(width: 20, height: 20)
                    .background(Color.white.opacity(0.06))
                    .clipShape(Circle())
            }
            .buttonStyle(.plain)
            .help(agent == nil ? "Close pane" : "Close pane (session stays in sidebar)")
        }
        .padding(.horizontal, 12).padding(.vertical, 9)
    }

    // MARK: Orb area

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

    // MARK: Metrics — Context (window pressure) · Activity · Cost

    private func metrics(_ agent: LiveAgent) -> some View {
        HStack(spacing: 14) {
            pressureBar(agent.pressure)
            metricLabel("Activity", "\(Int(agent.activity * 100))%")
            metricLabel("Cost", String(format: "$%.4f", agent.costUsd))
        }
        .padding(.horizontal, 12).padding(.vertical, 7)
        .background(Color.black.opacity(0.2))
    }

    /// Inline bar for how full the model's context window is, with a numeric readout.
    private func pressureBar(_ value: Double) -> some View {
        let warn = value > 0.75
        return HStack(spacing: 6) {
            Text("Context").font(paneMono(8)).foregroundStyle(Konjo.fgMute)
            GeometryReader { g in
                ZStack(alignment: .leading) {
                    Capsule().fill(Color.black.opacity(0.4))
                    Capsule()
                        .fill(warn ? Konjo.rose : Konjo.konjo2)
                        .frame(width: g.size.width * CGFloat(min(max(value, 0), 1)))
                        .animation(.easeOut(duration: 0.3), value: value)
                }
            }
            .frame(height: 4)
            Text("\(Int(value * 100))%")
                .font(paneMono(8))
                .foregroundStyle(warn ? Konjo.rose : Konjo.fgMute)
                .monospacedDigit()
        }
        .frame(maxWidth: .infinity)
    }

    private func metricLabel(_ key: String, _ value: String) -> some View {
        HStack(spacing: 4) {
            Text(key).font(paneMono(8)).foregroundStyle(Konjo.fgMute)
            Text(value).font(paneMono(8)).foregroundStyle(Konjo.fgDim).monospacedDigit()
        }
    }

    // MARK: Mini log strip (shown only when the full log panel is closed)

    private func logStrip(_ agent: LiveAgent) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            if agent.logTail.isEmpty {
                Text("— waiting for output —")
                    .font(paneMono(8)).italic().foregroundStyle(Konjo.fgMute)
            } else {
                ForEach(Array(agent.logTail.suffix(3).enumerated()), id: \.offset) { _, log in
                    HStack(spacing: 6) {
                        Text("[\(log.level.prefix(1).uppercased())]")
                            .foregroundStyle(logLevelColor(log.level))
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

    // MARK: Full log panel (right column, 1/3 width)

    private func logPanel(_ agent: LiveAgent) -> some View {
        VStack(spacing: 0) {
            logPanelHeader(agent)

            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 1) {
                        if agent.logTail.isEmpty {
                            HStack {
                                WaitingDots(accent: accent)
                                Spacer()
                            }
                            .padding(.vertical, 4)
                        } else {
                            ForEach(Array(agent.logTail.enumerated()), id: \.offset) { i, log in
                                logRow(log, index: i)
                                    .id(i)
                                    .transition(.asymmetric(
                                        insertion: .move(edge: .bottom).combined(with: .opacity),
                                        removal: .identity
                                    ))
                            }
                            // Live "waiting for Claude" indicator while the agent works.
                            if isLive {
                                WaitingDots(accent: accent)
                                    .padding(.top, 4)
                                    .id(-1)
                            }
                        }
                    }
                    .padding(20)
                    .animation(.easeOut(duration: 0.18), value: agent.logTail.count)
                }
                .onChange(of: agent.logTail.count) { _, count in
                    guard count > 0 else { return }
                    withAnimation(.easeOut(duration: 0.15)) {
                        proxy.scrollTo(count - 1, anchor: .bottom)
                    }
                }
            }
        }
        .background(Color.black.opacity(0.45))
    }

    private func logPanelHeader(_ agent: LiveAgent) -> some View {
        HStack(spacing: 6) {
            Image(systemName: "text.alignleft")
                .font(.system(size: 9, weight: .medium))
                .foregroundStyle(Konjo.fgMute)
            Text("CLAUDE OUTPUT")
                .font(paneMono(8, weight: .semibold))
                .tracking(1.2)
                .foregroundStyle(Konjo.fgMute)
            Spacer()
            iconButton("xmark", help: "Hide log panel") {
                withAnimation(.spring(response: 0.42, dampingFraction: 0.82)) { showLog = false }
            }
            iconButton("doc.on.doc", help: "Copy all output") {
                let all = agent.logTail.map { "[\($0.level.uppercased())] \($0.text)" }
                                       .joined(separator: "\n")
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(all, forType: .string)
            }
        }
        .padding(.horizontal, 10).padding(.vertical, 7)
        .overlay(Rectangle().fill(Konjo.line).frame(height: 1), alignment: .bottom)
    }

    private func iconButton(_ icon: String, help: String, _ action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: icon)
                .font(.system(size: 9, weight: .bold))
                .foregroundStyle(Konjo.fgMute)
                .frame(width: 18, height: 18)
                .background(Color.white.opacity(0.06))
                .clipShape(Circle())
        }
        .buttonStyle(.plain)
        .help(help)
    }

    private func logRow(_ log: AgentLog, index: Int) -> some View {
        HStack(alignment: .top, spacing: 6) {
            Text(log.level.prefix(1).uppercased())
                .font(.system(size: 8, weight: .bold, design: .monospaced))
                .foregroundStyle(logLevelColor(log.level))
                .frame(width: 10, alignment: .leading)
                .padding(.top, 1)
            MarkdownLogView(text: log.text, textColor: logTextColor(log.level))
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(.vertical, 2)
        .padding(.horizontal, 4)
        .background(index % 2 == 0 ? Color.clear : Color.white.opacity(0.02))
        .clipShape(RoundedRectangle(cornerRadius: 3))
    }

    private func logLevelColor(_ level: String) -> Color {
        switch level {
        case "error": return Konjo.rose
        case "warn":  return Konjo.flame
        default:      return Konjo.fgMute
        }
    }

    private func logTextColor(_ level: String) -> Color {
        switch level {
        case "error": return Konjo.rose.opacity(0.85)
        case "warn":  return Konjo.flame.opacity(0.9)
        default:      return Konjo.fgDim
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

    // MARK: Bottom bar — attempt · branch on the left, retry/stop on the right

    private func bottomBar(_ agent: LiveAgent) -> some View {
        HStack(spacing: 8) {
            Text("attempt \(agent.attempt)").font(paneMono(8)).foregroundStyle(Konjo.fgMute)
            if let branch = agent.branch {
                Text("· \(branch)").font(paneMono(8)).foregroundStyle(Konjo.fgMute).lineLimit(1)
            }
            Spacer(minLength: 8)
            barButton("arrow.clockwise", Konjo.sun, help: "Retry task") {
                submit(goal: agent.goal)
            }
            barButton("stop.fill", Konjo.rose, disabled: !agent.active, help: "Stop / cancel") {
                Task { await model.cancelTask(agent.id) }
            }
        }
        .padding(.horizontal, 12).padding(.vertical, 7)
    }

    /// Compact control for the bottom bar.
    private func barButton(_ icon: String, _ color: Color, disabled: Bool = false,
                           help: String, _ action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: icon)
                .font(.system(size: 12))
                .foregroundStyle(color)
                .frame(width: 30, height: 24)
                .background(RoundedRectangle(cornerRadius: 7).fill(color.opacity(0.06)))
                .overlay(RoundedRectangle(cornerRadius: 7).stroke(Konjo.line2, lineWidth: 1))
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
        withAnimation(.spring(response: 0.42, dampingFraction: 0.82)) {
            showLog = true
        }
        Task {
            await model.submitTask(controls.body(goal: trimmed))
            await MainActor.run {
                goal = ""
                submitting = false
            }
        }
    }
}

/// Three pulsing dots + label, shown while the pane waits on Claude. Driven by a
/// `TimelineView` so it animates smoothly regardless of surrounding state churn.
private struct WaitingDots: View {
    var accent: Color

    var body: some View {
        TimelineView(.animation) { tl in
            let t = tl.date.timeIntervalSinceReferenceDate
            HStack(spacing: 5) {
                ForEach(0..<3, id: \.self) { i in
                    Circle()
                        .fill(accent)
                        .frame(width: 5, height: 5)
                        .opacity(0.25 + 0.75 * pulse(t, phase: Double(i) * 0.22))
                }
                Text("waiting for Claude…")
                    .font(.system(size: 9, design: .monospaced))
                    .foregroundStyle(Konjo.fgMute)
                    .padding(.leading, 4)
            }
        }
    }

    /// 0…1 sinusoidal pulse, offset per dot to make the dots ripple.
    private func pulse(_ t: Double, phase: Double) -> Double {
        (sin((t * 2.2 - phase) * .pi) + 1) / 2
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
