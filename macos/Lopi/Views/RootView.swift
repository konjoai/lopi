import SwiftUI

/// Sidebar sections. Each maps to a live screen backed by the lopi REST/WS API.
enum NavSection: String, CaseIterable, Identifiable {
    case dashboard = "Dashboard"
    case tasks = "Tasks"
    case cron = "Cron"
    case constellations = "Constellations"
    case deadLetter = "Dead-Letter"
    case tools = "Tools"
    case health = "Health"
    case patterns = "Patterns"
    case audit = "Audit"
    case config = "Config"

    var id: String { rawValue }

    var icon: String {
        switch self {
        case .dashboard: return "gauge.with.dots.needle.67percent"
        case .tasks: return "list.bullet.rectangle"
        case .cron: return "clock.arrow.circlepath"
        case .constellations: return "circle.hexagongrid"
        case .deadLetter: return "tray.full"
        case .tools: return "wrench.and.screwdriver"
        case .health: return "heart.text.square"
        case .patterns: return "sparkles"
        case .audit: return "doc.text.magnifyingglass"
        case .config: return "gearshape.2"
        }
    }
}

struct RootView: View {
    @Environment(AppModel.self) private var model
    @State private var selection: NavSection? = .dashboard

    private var runningCount: Int { model.activeAgents.filter { $0.active }.count }

    var body: some View {
        NavigationSplitView {
            List(NavSection.allCases, selection: $selection) { section in
                Label {
                    HStack {
                        Text(section.rawValue)
                        Spacer()
                        if let badge = badge(for: section) {
                            Text(badge)
                                .font(Konjo.mono(9, weight: .semibold))
                                .foregroundStyle(Konjo.konjo2)
                                .padding(.horizontal, 6).padding(.vertical, 1)
                                .background(Konjo.konjo.opacity(0.18))
                                .clipShape(Capsule())
                        }
                    }
                } icon: {
                    Image(systemName: section.icon)
                }
                .tag(section)
            }
            .navigationSplitViewColumnWidth(min: 200, ideal: 220)
            .safeAreaInset(edge: .bottom) { sidebarFooter }
        } detail: {
            detail
                .background(Konjo.bg)
                .toolbar {
                    ToolbarItem(placement: .principal) {
                        ConnectionLED(state: model.connection)
                    }
                }
                .overlay(alignment: .top) { bannerOverlay }
        }
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
                    PulseOrb(color: Konjo.konjo, active: true).frame(width: 14, height: 14)
                    Text("\(runningCount) live")
                        .font(Konjo.mono(10)).foregroundStyle(Konjo.fgDim)
                }
            }
        }
        .padding(12)
    }

    @ViewBuilder private var detail: some View {
        switch selection ?? .dashboard {
        case .dashboard: DashboardView()
        case .tasks: TasksView()
        case .cron: CronView()
        case .constellations: ConstellationsView()
        case .deadLetter: DeadLetterView()
        case .tools: ToolsView()
        case .health: HealthView()
        case .patterns: PatternsView()
        case .audit: AuditView()
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
