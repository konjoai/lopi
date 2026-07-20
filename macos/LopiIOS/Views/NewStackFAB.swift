import SwiftUI

/// The "+ New Stack" floating action button on the Overview screen — matches
/// the design handoff's `FAB`: a flame-gradient pill pinned bottom-trailing.
/// Tapping it opens a freshly-created blank pane full-screen (see
/// `StackOverviewScreen.startNewStack`), same chrome as `StackDetailScreen`
/// since a brand-new pane already renders as "just the composer".
struct NewStackFAB: View {
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 8) {
                Image(systemName: "plus").font(.system(size: 15, weight: .bold))
                Text("New Stack").font(Konjo.sans(14, weight: .bold))
            }
            .foregroundStyle(Color(hex: 0x1A0F00))
            .padding(.horizontal, 20)
            .frame(height: 52)
            .background(
                LinearGradient(colors: [Konjo.flame, Color(hex: 0xE6820A)], startPoint: .top, endPoint: .bottom),
                in: Capsule()
            )
            .shadow(color: Konjo.flame.opacity(0.35), radius: 12, y: 8)
        }
        .buttonStyle(.plain)
    }
}
