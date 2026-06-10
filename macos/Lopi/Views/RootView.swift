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
    @EnvironmentObject private var model: AppModel
    @State private var selection: NavSection? = .dashboard

    var body: some View {
        NavigationSplitView {
            List(NavSection.allCases, selection: $selection) { section in
                Label(section.rawValue, systemImage: section.icon)
                    .font(Konjo.sans(13, weight: .medium))
                    .tag(section)
            }
            .scrollContentBackground(.hidden)
            .background(Konjo.deep)
            .tint(Konjo.ice)
            .navigationSplitViewColumnWidth(min: 180, ideal: 200)
            .safeAreaInset(edge: .bottom) {
                ConnectionLED(state: model.connection)
                    .padding(12)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        } detail: {
            detail
                .background(KonjoBackground())
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
        case .constellations: PlaceholderView(section: .constellations, endpoint: "/api/constellations")
        case .deadLetter: PlaceholderView(section: .deadLetter, endpoint: "/api/tasks/dead-letter")
        case .tools: PlaceholderView(section: .tools, endpoint: "/api/tools")
        case .health: PlaceholderView(section: .health, endpoint: "/api/agents/health/summary")
        case .patterns: PlaceholderView(section: .patterns, endpoint: "/api/patterns")
        case .audit: PlaceholderView(section: .audit, endpoint: "/api/audit")
        case .config: PlaceholderView(section: .config, endpoint: "/api/config")
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
