import SwiftUI

/// Read-only server config (secrets redacted server-side) plus result-cache
/// stats with a clear action.
struct ConfigView: View {
    @Environment(AppModel.self) private var model
    @State private var config: JSONValue = .null
    @State private var source = ""
    @State private var cache: CacheStatsModel?

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                appearancePanel
                cachePanel
                configPanel
            }
            .padding(20)
        }
        .background(Konjo.bg)
        .task { await reload() }
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button { Task { await reload() } } label: {
                    Label("Refresh", systemImage: "arrow.clockwise")
                }
            }
        }
    }

    private var appearancePanel: some View {
        KonjoPanel {
            VStack(alignment: .leading, spacing: 10) {
                Text("APPEARANCE")
                    .font(Konjo.mono(11))
                    .foregroundStyle(Konjo.fgMute)
                Text("accent theme · stored on this Mac only")
                    .font(Konjo.sans(11))
                    .foregroundStyle(Konjo.fgDim)
                HStack(spacing: 10) {
                    ForEach(AccentTheme.allCases) { theme in
                        themeSwatch(theme)
                    }
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private func themeSwatch(_ theme: AccentTheme) -> some View {
        let active = model.accentTheme == theme
        return Button { model.accentTheme = theme } label: {
            HStack(spacing: 8) {
                Circle().fill(theme.swatch).frame(width: 12, height: 12)
                    .shadow(color: theme.swatch.opacity(active ? 0.7 : 0), radius: 6)
                Text(theme.label.uppercased())
                    .font(Konjo.mono(10, weight: .semibold)).tracking(1.2)
                    .foregroundStyle(active ? Konjo.fg : Konjo.fgDim)
            }
            .padding(.horizontal, 12).padding(.vertical, 7)
            .background(active ? theme.swatch.opacity(0.12) : Color.white.opacity(0.03))
            .overlay(RoundedRectangle(cornerRadius: 7).stroke(active ? theme.swatch.opacity(0.55) : Konjo.line, lineWidth: 1))
            .clipShape(RoundedRectangle(cornerRadius: 7))
            .scaleEffect(active ? 1.03 : 1)
        }
        .buttonStyle(.plain)
        .animation(.easeOut(duration: 0.15), value: active)
    }

    private var cachePanel: some View {
        KonjoPanel {
            VStack(alignment: .leading, spacing: 10) {
                HStack {
                    Text("RESULT CACHE")
                        .font(Konjo.mono(11))
                        .foregroundStyle(Konjo.fgMute)
                    Spacer()
                    Button(role: .destructive) {
                        Task {
                            if await model.clearCache() { await reload() }
                        }
                    } label: {
                        Label("Clear", systemImage: "trash")
                    }
                    .buttonStyle(.borderless)
                }
                if let c = cache {
                    HStack(spacing: 24) {
                        metric("entries", "\(c.totalEntries)")
                        metric("size", byteString(c.totalSizeBytes))
                        metric("hit rate (1h)", "\(Int(c.hitRateLastHour * 100))%")
                        if let oldest = c.oldestEntry {
                            metric("oldest", DateFormatting.short(oldest))
                        }
                    }
                } else {
                    Text("Cache stats unavailable")
                        .font(Konjo.mono(11))
                        .foregroundStyle(Konjo.fgMute)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private var configPanel: some View {
        KonjoPanel {
            VStack(alignment: .leading, spacing: 10) {
                HStack {
                    Text("SERVER CONFIG")
                        .font(Konjo.mono(11))
                        .foregroundStyle(Konjo.fgMute)
                    Spacer()
                    Text("source: \(source)")
                        .font(Konjo.mono(10))
                        .foregroundStyle(Konjo.fgMute)
                }
                if case .null = config {
                    Text("No lopi.toml found on the server — defaults in effect.")
                        .font(Konjo.sans(12))
                        .foregroundStyle(Konjo.fgDim)
                } else {
                    Text(config.pretty())
                        .font(Konjo.mono(11))
                        .foregroundStyle(Konjo.fgDim)
                        .textSelection(.enabled)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private func metric(_ label: String, _ value: String) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(label.uppercased())
                .font(Konjo.mono(9))
                .foregroundStyle(Konjo.fgMute)
            Text(value)
                .font(Konjo.sans(16, weight: .semibold))
                .foregroundStyle(Konjo.fg)
        }
    }

    private func byteString(_ bytes: Int) -> String {
        ByteCountFormatter.string(fromByteCount: Int64(bytes), countStyle: .binary)
    }

    private func reload() async {
        if let tree = await model.configTree() {
            config = tree.config
            source = tree.source
        }
        cache = await model.cacheStats()
    }
}
