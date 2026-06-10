import SwiftUI

/// Consistent placeholder for admin screens not yet implemented. Names the
/// backing endpoint so the screen's intent is visible during development.
struct PlaceholderView: View {
    let section: NavSection
    let endpoint: String

    var body: some View {
        VStack(spacing: 14) {
            Image(systemName: section.icon)
                .font(.system(size: 40))
                .foregroundStyle(Konjo.konjo2)
            Text(section.rawValue)
                .font(Konjo.sans(22, weight: .semibold))
                .foregroundStyle(Konjo.fg)
            Text("Planned screen — backed by \(endpoint)")
                .font(Konjo.mono(12))
                .foregroundStyle(Konjo.fgMute)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Konjo.bg)
    }
}
