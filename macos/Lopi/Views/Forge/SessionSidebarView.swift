import SwiftUI

/// The sessions rail — every task the server knows about, whether mounted as a
/// pane or parked. Closing a pane drops a session here; the trash action is the
/// only path that permanently deletes one. Tombstoned sessions are filtered out
/// so a delete sticks across reconnects.
struct SessionSidebarView: View {
    @Environment(AppModel.self) private var model
    var layout: PaneLayout
    @Binding var collapsed: Bool

    private var sessions: [LiveAgent] {
        model.activeAgents.filter { !layout.isDeleted($0.id) }
    }

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider().overlay(Konjo.line)
            if !collapsed {
                ScrollView {
                    LazyVStack(spacing: 3) {
                        if sessions.isEmpty {
                            Text("no sessions yet")
                                .font(Konjo.mono(10)).foregroundStyle(Konjo.fgMute)
                                .padding(.vertical, 20)
                        }
                        ForEach(sessions) { row($0) }
                    }
                    .padding(6)
                }
            }
            Spacer(minLength: 0)
        }
        .frame(width: collapsed ? 40 : 240)
        .background(Konjo.bg.opacity(0.6))
        .overlay(alignment: .trailing) { Rectangle().fill(Konjo.line).frame(width: 1) }
    }

    private var header: some View {
        HStack(spacing: 8) {
            Button {
                withAnimation(.easeInOut(duration: 0.18)) { collapsed.toggle() }
            } label: {
                Image(systemName: collapsed ? "chevron.right" : "chevron.left")
                    .font(.system(size: 10, weight: .bold)).foregroundStyle(Konjo.konjo2)
            }
            .buttonStyle(.plain)
            if !collapsed {
                Text("SESSIONS").font(Konjo.mono(9, weight: .semibold)).tracking(1.6)
                    .foregroundStyle(Konjo.fgDim)
                Spacer()
                Text("\(sessions.count)").font(Konjo.mono(9)).foregroundStyle(Konjo.fgMute).monospacedDigit()
            }
        }
        .padding(.horizontal, 10).padding(.vertical, 10)
    }

    private func row(_ agent: LiveAgent) -> some View {
        let isOpen = layout.agentIsOpen(agent.id)
        return HStack(spacing: 2) {
            Button {
                layout.openSession(agent.id)
            } label: {
                HStack(spacing: 8) {
                    Circle().fill(statusColor(agent)).frame(width: 7, height: 7)
                    VStack(alignment: .leading, spacing: 1) {
                        Text(agent.goal).font(Konjo.mono(11)).lineLimit(1).foregroundStyle(Konjo.fg)
                        HStack(spacing: 4) {
                            Text(agent.phase.uppercased()).foregroundStyle(PhaseStyle.color(agent.phase))
                            if layout.isParked(agent.id) { Text("· parked").foregroundStyle(Konjo.fgMute) }
                        }
                        .font(Konjo.mono(8)).tracking(0.8)
                    }
                    Spacer(minLength: 0)
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)

            Button {
                delete(agent.id)
            } label: {
                Image(systemName: "trash").font(.system(size: 10)).foregroundStyle(Konjo.fgMute)
                    .frame(width: 24, height: 28)
            }
            .buttonStyle(.plain)
            .help("Delete session permanently")
        }
        .padding(.horizontal, 8).padding(.vertical, 6)
        .background(
            RoundedRectangle(cornerRadius: 8)
                .fill(isOpen ? Konjo.konjo.opacity(0.10) : Color.clear)
                .overlay(RoundedRectangle(cornerRadius: 8)
                    .stroke(isOpen ? Konjo.konjo.opacity(0.25) : Color.clear, lineWidth: 1))
        )
    }

    private func statusColor(_ a: LiveAgent) -> Color {
        if !a.active { return Konjo.fgMute }
        return PhaseStyle.color(a.phase)
    }

    private func delete(_ id: String) {
        layout.tombstone(id)
        model.liveAgents.removeValue(forKey: id)
        Task { await model.cancelTask(id) }
    }
}
