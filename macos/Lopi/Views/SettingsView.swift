import SwiftUI

/// Server connection settings. Host/port persist in UserDefaults; the token
/// goes to the Keychain. Saving reconnects the live stream.
struct SettingsView: View {
    @EnvironmentObject private var model: AppModel

    @State private var host = ""
    @State private var port = ""
    @State private var token = ""

    var body: some View {
        Form {
            Section("Server") {
                TextField("Host", text: $host)
                TextField("Port", text: $port)
                SecureField("Bearer token (optional)", text: $token)
            }
            Section("Status") {
                LabeledContent("Connection", value: connectionLabel)
                if let v = model.serverVersion {
                    LabeledContent("Server", value: "\(v.service) \(v.version)")
                    LabeledContent("Uptime", value: "\(v.uptimeSecs)s")
                }
            }
            HStack {
                Spacer()
                Button("Apply") { apply() }
                    .keyboardShortcut(.defaultAction)
            }
        }
        .formStyle(.grouped)
        .frame(width: 420)
        .padding()
        .onAppear(perform: prime)
    }

    private var connectionLabel: String {
        switch model.connection {
        case .live: return "Live"
        case .connecting: return "Connecting"
        case .offline: return "Offline"
        }
    }

    private func prime() {
        host = model.config.host
        port = String(model.config.port)
        token = model.config.token ?? ""
    }

    private func apply() {
        let cfg = ServerConfig(
            host: host.isEmpty ? "127.0.0.1" : host,
            port: Int(port) ?? 3000,
            token: token.isEmpty ? nil : token
        )
        model.updateConfig(cfg)
    }
}
