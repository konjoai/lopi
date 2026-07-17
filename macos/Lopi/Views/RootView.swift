import SwiftUI
import LopiStacksKit

/// Sidebar sections. Each maps to a live screen backed by the lopi REST/WS API.
enum NavSection: String, CaseIterable, Identifiable {
    case forge = "Loop Stack"
    case dashboard = "Dashboard"
    case budget = "Budget"
    case cron = "Cron"
    case loop = "Loop"
    case overview = "Overview"
    case config = "Config"

    var id: String { rawValue }

    var icon: String {
        switch self {
        case .forge: return "square.3.layers.3d"
        case .dashboard: return "gauge.with.dots.needle.67percent"
        case .budget: return "dollarsign.circle"
        case .cron: return "clock.arrow.circlepath"
        case .loop: return "arrow.triangle.2.circlepath"
        case .overview: return "list.bullet.rectangle"
        case .config: return "gearshape.2"
        }
    }
}

struct RootView: View {
    @Environment(AppModel.self) private var model
    @State private var selection: NavSection? = .forge
    // Shared with the Forge grid so the unified sidebar's session list drives
    // the same panes — one source of truth for the cockpit layout.
    @State private var layout = PaneLayout()

    private var runningCount: Int { model.activeAgents.filter { $0.active }.count }
    private var sessions: [LiveAgent] {
        model.activeAgents.filter { !layout.isDeleted($0.id) }
    }

    var body: some View {
        NavigationSplitView {
            sidebar
                .navigationSplitViewColumnWidth(min: 232, ideal: 252)
                .toolbarBackground(Konjo.bg, for: .windowToolbar)
                .toolbarBackground(.visible, for: .windowToolbar)
        } detail: {
            detail
                .background(Konjo.bg)
                // No system toolbar — the app draws its own black top bar, so
                // there's no reserved toolbar band leaving a gap above it.
                .overlay(alignment: .top) { bannerOverlay }
        }
    }

    // MARK: Unified sidebar — navigation + live sessions on the Konjo void.

    private var sidebar: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 2) {
                Color.clear.frame(height: 6)
                ForEach(NavSection.allCases) { navRow($0) }

                HStack(spacing: 6) {
                    sectionLabel("SESSIONS")
                    Spacer()
                    Text("\(sessions.count)")
                        .font(Konjo.mono(9)).foregroundStyle(Konjo.fgMute).monospacedDigit()
                }
                .padding(.trailing, 10)

                if sessions.isEmpty {
                    Text("no sessions yet")
                        .font(Konjo.mono(10)).foregroundStyle(Konjo.fgMute)
                        .padding(.horizontal, 10).padding(.vertical, 8)
                }
                ForEach(sessions) { sessionRow($0) }
            }
            .padding(8)
        }
        .scrollContentBackground(.hidden)
        .background(Konjo.bg)
        .safeAreaInset(edge: .bottom) { sidebarFooter }
    }

    private func sectionLabel(_ text: String) -> some View {
        Text(text)
            .font(Konjo.mono(9, weight: .semibold)).tracking(1.6)
            .foregroundStyle(Konjo.fgMute)
            .padding(.horizontal, 10).padding(.top, 14).padding(.bottom, 4)
    }

    private func navRow(_ section: NavSection) -> some View {
        let selected = (selection ?? .forge) == section
        return Button {
            // Clear any stale error banner when navigating. The banner is a
            // single shared slot with no auto-dismiss, so a decode/fetch error
            // raised on one section would otherwise stay pinned over every
            // other section's header until manually closed (the "sticky toast"
            // Ops-2 saw from the dead Constellations fetch). Removing that view
            // deletes the trigger; clearing here hardens the general case.
            model.banner = nil
            selection = section
        } label: {
            HStack(spacing: 10) {
                Group {
                    if section == .forge {
                        LopiLogoMark(size: 16)
                    } else {
                        Image(systemName: section.icon)
                            .font(.system(size: 13))
                            .foregroundStyle(selected ? model.accentColor : Konjo.fgDim)
                    }
                }
                .frame(width: 18)
                Text(section.rawValue)
                    .font(Konjo.sans(13, weight: selected ? .semibold : .regular))
                    .foregroundStyle(selected ? Konjo.fg : Konjo.fgDim)
                Spacer(minLength: 0)
                if let badge = badge(for: section) {
                    Text(badge)
                        .font(Konjo.mono(9, weight: .semibold)).foregroundStyle(Konjo.flame)
                        .padding(.horizontal, 6).padding(.vertical, 1)
                        .background(Konjo.flame.opacity(0.18)).clipShape(Capsule())
                }
            }
            .padding(.horizontal, 10).padding(.vertical, 7)
            .background(
                RoundedRectangle(cornerRadius: 8)
                    .fill(selected ? Konjo.flame.opacity(0.12) : .clear)
                    .overlay(RoundedRectangle(cornerRadius: 8)
                        .stroke(selected ? Konjo.flame.opacity(0.25) : .clear, lineWidth: 1))
            )
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    private func sessionRow(_ agent: LiveAgent) -> some View {
        let isOpen = layout.agentIsOpen(agent.id)
        return HStack(spacing: 2) {
            Button {
                selection = .forge
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
                    .frame(width: 22, height: 26)
            }
            .buttonStyle(.plain)
            .help("Delete session permanently")
        }
        .padding(.horizontal, 8).padding(.vertical, 6)
        .background(
            RoundedRectangle(cornerRadius: 8)
                .fill(isOpen ? Konjo.flame.opacity(0.10) : Color.clear)
                .overlay(RoundedRectangle(cornerRadius: 8)
                    .stroke(isOpen ? Konjo.flame.opacity(0.25) : Color.clear, lineWidth: 1))
        )
    }

    private func statusColor(_ a: LiveAgent) -> Color {
        a.active ? PhaseStyle.color(a.phase) : Konjo.fgMute
    }

    private func delete(_ id: String) {
        layout.tombstone(id)
        model.liveAgents.removeValue(forKey: id)
        Task { await model.cancelTask(id) }
    }

    /// Live count badges so the whole nav reflects backend state at a glance.
    private func badge(for section: NavSection) -> String? {
        switch section {
        case .dashboard where runningCount > 0: return "\(runningCount)"
        case .cron where !model.schedules.isEmpty: return "\(model.schedules.count)"
        default: return nil
        }
    }

    private var sidebarFooter: some View {
        HStack(spacing: 8) {
            ConnectionLED(state: model.connection)
            Spacer()
            if runningCount > 0 {
                HStack(spacing: 5) {
                    PulseOrb(color: model.accentColor, active: true).frame(width: 14, height: 14)
                    Text("\(runningCount) live")
                        .font(Konjo.mono(10)).foregroundStyle(Konjo.fgDim)
                }
            }
        }
        .padding(12)
        .background(Konjo.bg)
    }

    @ViewBuilder private var detail: some View {
        switch selection ?? .forge {
        case .forge: ForgeView()
        case .dashboard: DashboardView()
        case .budget: BudgetView()
        case .cron: CronView()
        case .loop: LoopView()
        case .overview: OverviewView(onFocus: { id in
            model.banner = nil
            selection = .forge
            layout.openSession(id)
        })
        case .config: ConfigView()
        }
    }

    @ViewBuilder private var bannerOverlay: some View {
        if let banner = model.banner {
            BannerView(text: banner) { model.banner = nil }
                .padding()
                .transition(.move(edge: .top).combined(with: .opacity))
        }
    }
}
