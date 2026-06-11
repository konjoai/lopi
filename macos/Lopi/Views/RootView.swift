import SwiftUI

/// Sidebar sections. Implemented screens render their views; the rest show a
/// consistent "coming soon" placeholder mapped to their backing endpoint.
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

    var body: some View {
        NavigationSplitView {
            List(NavSection.allCases, selection: $selection) { section in
                Label(section.rawValue, systemImage: section.icon)
                    .tag(section)
            }
            .navigationSplitViewColumnWidth(min: 180, ideal: 200)
            .safeAreaInset(edge: .bottom) {
                ConnectionLED(state: model.connection)
                    .padding(12)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
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
