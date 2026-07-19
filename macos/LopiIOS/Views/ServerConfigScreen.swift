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
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss

    @State private var host = ""
    @State private var port = ""
    @State private var token = ""

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
                            statusRow("uptime", "\(v.uptimeSecs)s")
                        }
                    }

                    Button(action: apply) {
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
                }
                .padding(16)
            }
            .background(Konjo.panel)
            .navigationTitle("server")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("done") { dismiss() }
                }
            }
        }
        .onAppear(perform: prime)
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
