import SwiftUI

/// Custom branded sidebar matching the web nav: a `lopi · forge` wordmark,
/// ice-tinted active rows (the native `List` selection ignores the asset
/// accent and stays system-blue), and a live connection LED pinned bottom.
struct KonjoSidebar: View {
    @Binding var selection: NavSection
    let connection: ConnectionState

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            brand
            ScrollView {
                VStack(spacing: 2) {
                    ForEach(NavSection.allCases) { row($0) }
                }
                .padding(.horizontal, 8)
                .padding(.top, 4)
            }
            ConnectionLED(state: connection)
                .padding(14)
        }
        .frame(maxHeight: .infinity, alignment: .top)
        .background(Konjo.deep)
    }

    private var brand: some View {
        HStack(spacing: 7) {
            Text("lopi")
                .font(Konjo.sans(20, weight: .bold))
                .foregroundStyle(Konjo.paper)
            Text("· forge")
                .font(Konjo.mono(9)).tracking(2)
                .foregroundStyle(Konjo.fgMute)
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 16)
        .padding(.top, 16).padding(.bottom, 12)
    }

    private func row(_ section: NavSection) -> some View {
        let active = selection == section
        return Button { selection = section } label: {
            HStack(spacing: 10) {
                Image(systemName: section.icon)
                    .font(.system(size: 13, weight: .medium))
                    .frame(width: 18)
                    .foregroundStyle(active ? Konjo.ice : Konjo.fgDim)
                Text(section.rawValue)
                    .font(Konjo.sans(13, weight: active ? .semibold : .regular))
                    .foregroundStyle(active ? Konjo.paper : Konjo.fgDim)
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 10).padding(.vertical, 7)
            .background(active ? Konjo.ice.opacity(0.12) : Color.clear)
            .overlay(alignment: .leading) {
                if active {
                    Capsule().fill(Konjo.ice).frame(width: 2.5, height: 15)
                }
            }
            .clipShape(RoundedRectangle(cornerRadius: 7))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}
