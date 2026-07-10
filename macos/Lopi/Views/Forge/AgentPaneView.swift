import SwiftUI

/// One pane in the Forge grid — a full-pane chat: a header, a transcript that
/// fills the body, and a composer pinned at the bottom. The living orb is
/// absorbed into the bottom-right corner of the transcript (large + centered as
/// the launcher when idle; it travels + shrinks into the corner via
/// `matchedGeometryEffect` the moment a session goes live, then keeps animating
/// per the ORB STATE MAP). The mirror of the web AgentPane.
///
/// NOTE: written to mirror the verified web implementation; this macOS target was
/// not compiled in the authoring environment (Linux) — build on the M3.
struct AgentPaneView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    var agent: LiveAgent?
    var controls: LaunchControls
    /// Total panes in the grid (kept for API compatibility; orb now self-sizes).
    var paneCount: Int = 1
    var onClose: () -> Void

    @State private var goal = ""
    @State private var submitting = false
    @State private var deciding = false
    @Namespace private var orbNS

    // MARK: Derived state

    private var orb: ForgeOrbState { OrbStateMap.compute(agent, awaiting: agent?.awaitingApproval ?? false) }
    /// Drive the pane chrome from the orb's live state color (one voice).
    private var accent: Color { agent == nil ? Konjo.konjo : orb.glowColor }
    private var isLive: Bool { agent?.active ?? false }
    private var blocks: [TranscriptBlock] { agent.map { TranscriptBuilder.build(from: $0) } ?? [] }

    private var phaseLabel: String {
        guard let agent else { return "—" }
        return agent.awaitingApproval ? "Review" : agent.phase.capitalized
    }

    private static let textScale: CGFloat = 1.25
    private func paneMono(_ size: CGFloat, weight: Font.Weight = .regular) -> Font {
        Konjo.mono(size * Self.textScale, weight: weight)
    }
    private func paneSans(_ size: CGFloat, weight: Font.Weight = .regular) -> Font {
        Konjo.sans(size * Self.textScale, weight: weight)
    }

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider().overlay(Konjo.line)
            bodyArea
            Divider().overlay(Konjo.line)
            if let agent { metrics(agent) }
            composer
            if let agent { bottomBar(agent) }
        }
        .background(Konjo.bg1.opacity(0.6))
        .clipShape(RoundedRectangle(cornerRadius: 10))
        .overlay(
            RoundedRectangle(cornerRadius: 10)
                .stroke(isLive ? accent.opacity(0.35) : Konjo.line, lineWidth: 1)
        )
        .shadow(color: .black.opacity(0.55), radius: 14, y: 6)
        .shadow(color: isLive ? accent.opacity(0.28) : .clear, radius: 18)
        .animation(.easeInOut(duration: 0.4), value: isLive)
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

    // MARK: Body — transcript fills the pane; orb absorbed into the corner

    private var bodyArea: some View {
        GeometryReader { geo in
            // Orb-parity (Polish-1 §6): standardize on the COMPACT per-pane orb,
            // mirroring web's per-card `OrbDot` (a small status dot). The old
            // 120–300pt corner orb was the single-hero Forge orb, which doesn't
            // scale once several panes are visible at once — the divergence Ops-2
            // captured (web = small dot per card, macOS = large orb per pane).
            // The live orb is now a compact, still-animated status indicator; the
            // idle launcher below stays a larger hero because it's the
            // single-pane launch affordance, not the crowded multipane grid.
            let cornerSize = min(40, max(22, min(geo.size.width, geo.size.height) * 0.1))
            let idleSize = min(320, max(150, min(geo.size.width, geo.size.height) * 0.5))
            ZStack(alignment: .bottomTrailing) {
                if let agent {
                    TranscriptView(blocks: blocks, streaming: agent.active, orbInset: cornerSize + 16)
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                    // Corner orb overlay (non-interactive).
                    orbView(size: cornerSize)
                        .matchedGeometryEffect(id: "orb", in: orbNS)
                        .padding(10)
                        .allowsHitTesting(false)
                } else {
                    // Idle launcher: orb large + centered, controls beneath.
                    VStack(spacing: 16) {
                        orbView(size: idleSize)
                            .matchedGeometryEffect(id: "orb", in: orbNS)
                        LaunchControlsView(controls: controls, dense: true)
                            .padding(.horizontal, 14)
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                }
            }
            .overlay {
                if let agent, agent.awaitingApproval { planGate(agent) }
            }
            .animation(reduceMotion ? nil : .spring(response: 0.4, dampingFraction: 0.85), value: agent != nil)
        }
    }

    private func orbView(size: CGFloat) -> some View {
        KonjoOrb(
            phase: agent?.phase ?? "idle",
            activity: agent?.activity ?? 0,
            pressure: agent?.pressure ?? 0,
            health: agent?.testPassRate ?? 0.85,
            stimulus: agent?.stimulus ?? .distantPast,
            stimulusKind: agent?.stimulusKind ?? "request",
            size: size,
            glowColor: orb.glowColor,
            spinSpeed: orb.spinSpeed,
            pulseRate: orb.pulseRate,
            glowIntensity: orb.glowIntensity,
            turbulence: orb.turbulence,
            special: orb.special
        )
        .background(
            Circle()
                .fill(orb.glowColor.opacity(agent == nil ? 0.05 : 0.16))
                .frame(width: size * 1.24, height: size * 1.24)
                .blur(radius: 26)
        )
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

    // MARK: Composer — prompt input + send (Enter sends)

    private var composer: some View {
        HStack(spacing: 8) {
            Text(">").font(paneMono(16, weight: .medium)).foregroundStyle(Konjo.ok)
            TextField(agent == nil ? "type a goal…" : "message this agent…", text: $goal, axis: .vertical)
                .textFieldStyle(.plain)
                .lineLimit(1...5)
                .font(paneMono(15)).foregroundStyle(Konjo.fg)
                .onSubmit { submit(goal: goal) }
            if submitting {
                ProgressView().controlSize(.small)
            } else {
                Button { submit(goal: goal) } label: {
                    Image(systemName: "arrow.up")
                        .font(.system(size: 12, weight: .bold))
                        .foregroundStyle(Konjo.bg)
                        .frame(width: 26, height: 26)
                        .background(Konjo.ice)
                        .clipShape(RoundedRectangle(cornerRadius: 8))
                }
                .buttonStyle(.plain)
                .disabled(goal.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                .help("Send")
            }
        }
        .padding(.horizontal, 12).padding(.vertical, 9)
        .background(Color.black.opacity(0.18))
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
