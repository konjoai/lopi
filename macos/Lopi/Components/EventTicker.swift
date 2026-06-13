import SwiftUI

/// A live, animated event feed. New rows slide in from the top and the list
/// stays scrolled to the freshest event.
struct EventTicker: View {
    let items: [FeedItem]
    var maxRows: Int = 12

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if items.isEmpty {
                Text("Waiting for live events…")
                    .font(Konjo.mono(11))
                    .foregroundStyle(Konjo.fgMute)
                    .padding(.vertical, 8)
            } else {
                ForEach(Array(items.prefix(maxRows))) { item in
                    row(item)
                        .transition(.asymmetric(
                            insertion: .move(edge: .top).combined(with: .opacity),
                            removal: .opacity
                        ))
                }
            }
        }
        .animation(.spring(response: 0.4, dampingFraction: 0.8), value: items.first?.id)
    }

    private func row(_ item: FeedItem) -> some View {
        HStack(spacing: 10) {
            Image(systemName: item.kind.icon)
                .font(.system(size: 12))
                .foregroundStyle(item.kind.color)
                .frame(width: 18)
            VStack(alignment: .leading, spacing: 1) {
                Text(item.title)
                    .font(Konjo.mono(11, weight: .medium))
                    .foregroundStyle(Konjo.fg)
                    .lineLimit(1)
                Text(item.detail)
                    .font(Konjo.mono(10))
                    .foregroundStyle(Konjo.fgMute)
                    .lineLimit(1)
            }
            Spacer()
            Text(RelativeTime.short(item.at))
                .font(Konjo.mono(9))
                .foregroundStyle(Konjo.fgMute)
                .monospacedDigit()
        }
        .padding(.vertical, 6)
        .overlay(alignment: .bottom) {
            Rectangle().fill(Konjo.line).frame(height: 1)
        }
    }
}

/// Compact relative-time formatting for the ticker ("now", "12s", "4m").
enum RelativeTime {
    static func short(_ date: Date) -> String {
        let secs = max(0, Int(Date.now.timeIntervalSince(date)))
        switch secs {
        case 0..<3: return "now"
        case 3..<60: return "\(secs)s"
        case 60..<3600: return "\(secs / 60)m"
        default: return "\(secs / 3600)h"
        }
    }
}
