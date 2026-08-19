import SwiftUI
import LopiStacksKit

/// Server connection settings — host/port/bearer token. The iOS counterpart
/// to macOS's `SettingsView.swift` (a `Settings`-scene `Form`, which has no
/// iOS analogue), reusing the exact same persistence/reconnect calls
/// (`ServerConfig.load()`/`.save()`, `AppModel.updateConfig(_:)`) that view
/// demonstrates. Presented as a sheet from the Overview screen rather than a
/// forced first-run flow — `ServerConfig.load()` already falls back to a
/// usable default (127.0.0.1:3000), so there's no reliable "never configured"
/// signal to gate on, and a persistent settings entry point is more robust
/// than fragile first-run detection.
struct ServerConfigScreen: View {
    private enum ConfigViewMode: String, CaseIterable { case tree, raw }

    @Environment(AppModel.self) private var model

    @State private var host = ""
    @State private var port = ""
    @State private var token = ""
    @State private var effectiveConfig: JSONValue = .null
    @State private var configSource = ""
    @State private var cache: CacheStatsModel?
    @State private var configViewMode: ConfigViewMode = .tree

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    section("SERVER") {
                        field("host", text: $host, placeholder: "127.0.0.1")
                        field("port", text: $port, placeholder: "3000", keyboard: .numberPad)
                        secureField("bearer token (optional)", text: $token)
                    }

                    section("STATUS") {
                        statusRow("connection", connectionLabel, color: connectionColor)
                        if let v = model.serverVersion {
                            statusRow("server", "\(v.service) \(v.version)")
                            statusRow("uptime", Uptime.string(v.uptimeSecs))
                        }
                    }

                    Button(action: { Haptics.impact(); apply() }) {
                        Text("save & reconnect")
                            .font(Konjo.sans(14, weight: .bold))
                            .foregroundStyle(Color(hex: 0x1A0F00))
                            .frame(maxWidth: .infinity)
                            .padding(.vertical, 12)
                            .background(
                                LinearGradient(colors: [Konjo.flame, Color(hex: 0xE6820A)], startPoint: .top, endPoint: .bottom),
                                in: RoundedRectangle(cornerRadius: 10)
                            )
                    }
                    .buttonStyle(.plain)

                    section("APPEARANCE") {
                        Text("accent theme · stored on this device only")
                            .font(Konjo.sans(11)).foregroundStyle(Konjo.fgDim)
                        HStack(spacing: 8) {
                            ForEach(AccentTheme.allCases) { theme in
                                themeSwatch(theme)
                            }
                        }
                    }

                    section("RESULT CACHE") {
                        if let c = cache {
                            HStack(spacing: 20) {
                                metric("entries", "\(c.totalEntries)")
                                metric("size", byteString(c.totalSizeBytes))
                                metric("hit rate", "\(Int(c.hitRateLastHour * 100))%")
                            }
                            Button {
                                Haptics.warning()
                                Task { if await model.clearCache() { await reloadConfig() } }
                            } label: {
                                Text("clear cache")
                                    .font(Konjo.mono(10.5, weight: .semibold)).foregroundStyle(Konjo.rose)
                            }
                            .buttonStyle(.plain)
                        } else {
                            Text("cache stats unavailable").font(Konjo.mono(11)).foregroundStyle(Konjo.fgMute)
                        }
                    }

                    section("SERVER CONFIG") {
                        HStack {
                            Text("source: \(configSource.isEmpty ? "—" : configSource)")
                                .font(Konjo.mono(10)).foregroundStyle(Konjo.fgMute)
                            Spacer()
                            configModeToggle
                        }
                        if case .null = effectiveConfig {
                            Text("No lopi.toml found on the server — defaults in effect.")
                                .font(Konjo.sans(12)).foregroundStyle(Konjo.fgDim)
                        } else if configViewMode == .raw {
                            Text(effectiveConfig.pretty())
                                .font(Konjo.mono(10.5)).foregroundStyle(Konjo.fgDim)
                                .textSelection(.enabled)
                                .frame(maxWidth: .infinity, alignment: .leading)
                        } else {
                            configTree
                        }
                    }
                }
                .padding(16)
            }
            .background(Konjo.panel)
            .navigationTitle("server")
        }
        .onAppear(perform: prime)
        .task { await reloadConfig() }
    }

    private var connectionLabel: String {
        switch model.connection {
        case .live: return "live"
        case .connecting: return "connecting…"
        case .offline: return "offline"
        }
    }

    private var connectionColor: Color {
        switch model.connection {
        case .live: return Konjo.jade
        case .connecting: return Konjo.sun
        case .offline: return Konjo.fgMute
        }
    }

    private func prime() {
        host = model.config.host
        port = String(model.config.port)
        token = model.config.token ?? ""
    }

    private func apply() {
        let cfg = ServerConfig(
            host: host.trimmingCharacters(in: .whitespaces).isEmpty ? "127.0.0.1" : host,
            port: Int(port) ?? 3000,
            token: token.isEmpty ? nil : token
        )
        model.updateConfig(cfg)
    }

    private func reloadConfig() async {
        if let tree = await model.configTree() {
            effectiveConfig = tree.config
            configSource = tree.source
        }
        cache = await model.cacheStats()
    }

    private func byteString(_ bytes: Int) -> String {
        ByteCountFormatter.string(fromByteCount: Int64(bytes), countStyle: .binary)
    }

    private func metric(_ label: String, _ value: String) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(label.uppercased()).font(Konjo.mono(9)).foregroundStyle(Konjo.fgMute)
            Text(value).font(Konjo.sans(14, weight: .semibold)).foregroundStyle(Konjo.fg)
        }
    }

    // MARK: - Appearance

    private func themeSwatch(_ theme: AccentTheme) -> some View {
        let active = model.accentTheme == theme
        return Button {
            Haptics.selection()
            model.accentTheme = theme
        } label: {
            HStack(spacing: 6) {
                Circle().fill(theme.swatch).frame(width: 10, height: 10)
                    .shadow(color: theme.swatch.opacity(active ? 0.7 : 0), radius: 5)
                Text(theme.label.uppercased())
                    .font(Konjo.mono(9.5, weight: .semibold)).tracking(1)
                    .foregroundStyle(active ? Konjo.fg : Konjo.fgDim)
            }
            .padding(.horizontal, 10).padding(.vertical, 6)
            .background(active ? theme.swatch.opacity(0.12) : Color.white.opacity(0.03))
            .overlay(RoundedRectangle(cornerRadius: 7).stroke(active ? theme.swatch.opacity(0.55) : Konjo.line2, lineWidth: 1))
            .clipShape(RoundedRectangle(cornerRadius: 7))
        }
        .buttonStyle(.plain)
    }

    // MARK: - Effective config tree

    private var configModeToggle: some View {
        HStack(spacing: 2) {
            ForEach(ConfigViewMode.allCases, id: \.self) { mode in
                Button {
                    Haptics.selection()
                    configViewMode = mode
                } label: {
                    Text(mode.rawValue)
                        .font(Konjo.mono(9, weight: .semibold)).tracking(0.6)
                        .foregroundStyle(configViewMode == mode ? Konjo.ice : Konjo.fgMute)
                }
                .buttonStyle(.plain)
                .padding(.horizontal, 7).padding(.vertical, 3)
                .overlay(RoundedRectangle(cornerRadius: 5).stroke(configViewMode == mode ? Konjo.ice.opacity(0.5) : Konjo.line2, lineWidth: 1))
            }
        }
    }

    /// Flattened path/value leaf rows, color-coded by inferred type — same
    /// port as macOS's `ConfigView.configTree`/`flatten`/`configValueColor`.
    private var configTree: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(flatten(effectiveConfig).enumerated()), id: \.offset) { _, row in
                VStack(alignment: .leading, spacing: 2) {
                    Text(row.0).font(Konjo.mono(9.5)).foregroundStyle(Konjo.fgMute)
                    Text(row.1).font(Konjo.mono(11)).foregroundStyle(configValueColor(row.1))
                        .textSelection(.enabled)
                }
                .padding(.vertical, 5)
                Rectangle().fill(Konjo.line.opacity(0.4)).frame(height: 1)
            }
        }
    }

    private func configValueColor(_ value: String) -> Color {
        if value == "***" { return Konjo.rose }
        if value == "true" || value == "false" { return Konjo.sun }
        if Double(value) != nil { return Konjo.jade }
        return Konjo.fgDim
    }

    private func flatten(_ value: JSONValue, prefix: String = "") -> [(String, String)] {
        switch value {
        case .null: return [(prefix, "null")]
        case let .bool(b): return [(prefix, b ? "true" : "false")]
        case let .number(n):
            let text = n == n.rounded() && abs(n) < 1e15 ? String(Int(n)) : String(n)
            return [(prefix, text)]
        case let .string(s): return [(prefix, s)]
        case let .array(items):
            if items.isEmpty { return [(prefix, "[]")] }
            return items.enumerated().flatMap { i, v in flatten(v, prefix: "\(prefix)[\(i)]") }
        case let .object(obj):
            if obj.isEmpty { return [(prefix, "{}")] }
            return obj.keys.sorted().flatMap { key in
                flatten(obj[key, default: .null], prefix: prefix.isEmpty ? key : "\(prefix).\(key)")
            }
        }
    }

    // MARK: - Chrome

    private func section<Content: View>(_ title: String, @ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(title).font(Konjo.mono(9, weight: .bold)).tracking(1).foregroundStyle(Konjo.fgMute)
            VStack(alignment: .leading, spacing: 10) { content() }
        }
    }

    private func field(_ label: String, text: Binding<String>, placeholder: String, keyboard: UIKeyboardType = .default) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(label).font(Konjo.mono(10.5)).foregroundStyle(Konjo.fgDim)
            TextField(placeholder, text: text)
                .keyboardType(keyboard)
                .autocorrectionDisabled()
                .textInputAutocapitalization(.never)
                .font(Konjo.sans(13))
                .foregroundStyle(Konjo.fg)
                .padding(9)
                .background(Color.white.opacity(0.02))
                .clipShape(RoundedRectangle(cornerRadius: 7))
                .overlay(RoundedRectangle(cornerRadius: 7).stroke(Konjo.line2, lineWidth: 1))
        }
    }

    private func secureField(_ label: String, text: Binding<String>) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(label).font(Konjo.mono(10.5)).foregroundStyle(Konjo.fgDim)
            SecureField("", text: text)
                .autocorrectionDisabled()
                .textInputAutocapitalization(.never)
                .font(Konjo.sans(13))
                .foregroundStyle(Konjo.fg)
                .padding(9)
                .background(Color.white.opacity(0.02))
                .clipShape(RoundedRectangle(cornerRadius: 7))
                .overlay(RoundedRectangle(cornerRadius: 7).stroke(Konjo.line2, lineWidth: 1))
        }
    }

    private func statusRow(_ label: String, _ value: String, color: Color = .clear) -> some View {
        HStack {
            Text(label).font(Konjo.mono(10.5)).foregroundStyle(Konjo.fgDim)
            Spacer()
            HStack(spacing: 5) {
                if color != .clear {
                    Circle().fill(color).frame(width: 6, height: 6)
                }
                Text(value).font(Konjo.mono(11)).foregroundStyle(Konjo.fg)
            }
        }
    }
}
