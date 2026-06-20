import SwiftUI

/// One pane in the Forge grid: the orb, live metrics, and a command line.
/// An empty pane becomes a launcher (selectors + goal field); a live pane shows
/// cognition and exposes retry / stop / close-pane controls.
struct AgentPaneView: View {
    @Environment(AppModel.self) private var model
    var agent: LiveAgent?
    var controls: LaunchControls
    var onClose: () -> Void

    @State private var goal = ""
    @State private var submitting = false

    private var accent: Color { agent.map { PhaseStyle.color($0.phase) } ?? Konjo.konjo }
    private var isLive: Bool { agent.map { PhaseStyle.isActive($0.phase) && $0.active } ?? false }

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider().overlay(Konjo.line)
            ScrollView { centerColumn.padding(.vertical, 14) }
            if let agent { metrics(agent) }
            commandBar
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

    // MARK: Header

    private var header: some View {
        HStack(spacing: 8) {
            Circle().fill(accent).frame(width: 7, height: 7)
                .shadow(color: isLive ? accent.opacity(0.9) : .clear, radius: 5)
            VStack(alignment: .leading, spacing: 1) {
                Text(agent?.goal ?? "— idle —")
                    .font(Konjo.mono(11, weight: .medium)).lineLimit(1)
                    .foregroundStyle(agent == nil ? Konjo.fgMute : Konjo.fg)
                if let agent {
                    Text(agent.phase.uppercased())
                        .font(Konjo.mono(8)).tracking(1.2).foregroundStyle(accent)
                }
            }
            Spacer()
            Button(action: onClose) {
                Image(systemName: "xmark").font(.system(size: 9, weight: .bold))
                    .foregroundStyle(Konjo.fgDim)
                    .frame(width: 18, height: 18)
                    .background(Color.white.opacity(0.08))
                    .clipShape(Circle())
            }
            .buttonStyle(.plain)
            .help(agent == nil ? "Close pane" : "Close pane (session stays in sidebar)")
        }
        .padding(.horizontal, 12).padding(.vertical, 9)
    }

    // MARK: Center

    @ViewBuilder private var centerColumn: some View {
        VStack(spacing: 16) {
            KonjoOrb(
                phase: agent?.phase ?? "idle",
                activity: agent?.activity ?? 0,
                pressure: agent?.pressure ?? 0,
                size: 132
            )
            // Phase-tinted aura pooled behind the orb; breathes while live.
            .background(
                Circle()
                    .fill(accent.opacity(agent == nil ? 0.05 : 0.16))
                    .frame(width: 168, height: 168)
                    .blur(radius: 26)
            )
            if agent == nil {
                LaunchControlsView(controls: controls, dense: true)
                    .padding(.horizontal, 14)
            } else if let agent {
                controlsRow(agent)
            }
        }
    }

    private func controlsRow(_ agent: LiveAgent) -> some View {
        HStack(spacing: 10) {
            Button {
                submit(goal: agent.goal)
            } label: {
                Label("Retry", systemImage: "arrow.clockwise")
            }
            .konjoButton(Konjo.sun)
            Button {
                Task { await model.cancelTask(agent.id) }
            } label: {
                Label("Stop", systemImage: "stop.fill")
            }
            .konjoButton(Konjo.rose)
            .disabled(!agent.active)
            .opacity(agent.active ? 1 : 0.4)
        }
    }

    // MARK: Metrics

    private func metrics(_ agent: LiveAgent) -> some View {
        HStack(spacing: 12) {
            meter("P", value: agent.pressure, warn: agent.pressure > 0.75)
            label("A", "\(Int(agent.activity * 100))")
            label("$", String(format: "%.4f", agent.costUsd))
            if agent.attempt > 0 { label("#", "\(agent.attempt)") }
        }
        .padding(.horizontal, 12).padding(.vertical, 7)
        .background(Color.black.opacity(0.2))
    }

    private func meter(_ k: String, value: Double, warn: Bool) -> some View {
        HStack(spacing: 5) {
            Text("\(k):").font(Konjo.mono(9)).foregroundStyle(Konjo.fgMute)
            GeometryReader { g in
                ZStack(alignment: .leading) {
                    Capsule().fill(Color.black.opacity(0.4))
                    Capsule().fill(warn ? Konjo.err : Konjo.konjo2)
                        .frame(width: g.size.width * CGFloat(min(max(value, 0), 1)))
                }
            }
            .frame(height: 4)
        }
        .frame(maxWidth: 90)
    }

    private func label(_ k: String, _ v: String) -> some View {
        HStack(spacing: 4) {
            Text("\(k):").font(Konjo.mono(9)).foregroundStyle(Konjo.fgMute)
            Text(v).font(Konjo.mono(9)).foregroundStyle(Konjo.fgDim).monospacedDigit()
        }
    }

    // MARK: Command bar

    private var commandBar: some View {
        HStack(spacing: 8) {
            Text(">").font(Konjo.mono(11)).foregroundStyle(Konjo.ok)
            TextField(agent == nil ? "type a goal…" : "new goal…", text: $goal)
                .textFieldStyle(.plain)
                .font(Konjo.mono(11)).foregroundStyle(Konjo.fg)
                .onSubmit { submit(goal: goal) }
            if submitting {
                ProgressView().controlSize(.small)
            }
        }
        .padding(.horizontal, 12).padding(.vertical, 9)
        .background(Color.black.opacity(0.1))
    }

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
