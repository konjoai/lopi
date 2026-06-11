import SwiftUI

/// Durable tool registry: list, register, delete. Parameter schemas are
/// arbitrary JSON, edited as text and validated client-side before POST.
struct ToolsView: View {
    @Environment(AppModel.self) private var model
    @State private var rows: [ToolModel] = []
    @State private var loaded = false
    @State private var showRegister = false

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                if loaded && rows.isEmpty {
                    EmptyHint(icon: "wrench.and.screwdriver", text: "No tools registered.")
                }
                ForEach(rows) { tool in
                    card(tool)
                }
            }
            .padding(20)
        }
        .background(Konjo.bg)
        .task { await reload() }
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button { showRegister = true } label: {
                    Label("Register Tool", systemImage: "plus")
                }
            }
        }
        .sheet(isPresented: $showRegister) {
            RegisterToolSheet { body in
                if await model.registerTool(body) { await reload() }
            }
        }
    }

    private func card(_ tool: ToolModel) -> some View {
        KonjoPanel {
            VStack(alignment: .leading, spacing: 8) {
                HStack {
                    Text(tool.name)
                        .font(Konjo.mono(13, weight: .semibold))
                        .foregroundStyle(Konjo.konjo2)
                    Spacer()
                    Text("\(tool.timeoutMs)ms · \(tool.retries) retries")
                        .font(Konjo.mono(10))
                        .foregroundStyle(Konjo.fgMute)
                    Button(role: .destructive) {
                        Task {
                            if await model.deleteTool(tool.name) { await reload() }
                        }
                    } label: {
                        Image(systemName: "trash")
                    }
                    .buttonStyle(.borderless)
                }
                Text(tool.description)
                    .font(Konjo.sans(12))
                    .foregroundStyle(Konjo.fgDim)
                DisclosureGroup("parameters") {
                    Text(tool.parameters.pretty())
                        .font(Konjo.mono(10))
                        .foregroundStyle(Konjo.fgDim)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .textSelection(.enabled)
                }
                .font(Konjo.mono(10))
                .foregroundStyle(Konjo.fgMute)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private func reload() async {
        rows = await model.tools()
        loaded = true
    }
}

/// Sheet for registering a new tool spec.
struct RegisterToolSheet: View {
    @Environment(\.dismiss) private var dismiss
    let onSubmit: (RegisterToolBody) async -> Void

    @State private var name = ""
    @State private var descriptionText = ""
    @State private var parametersText = "{}"
    @State private var parseError: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Register Tool")
                .font(Konjo.sans(18, weight: .semibold))
                .foregroundStyle(Konjo.fg)
            TextField("Name (kebab-case)", text: $name)
                .textFieldStyle(.roundedBorder)
                .font(Konjo.mono(12))
            TextField("Description", text: $descriptionText, axis: .vertical)
                .lineLimit(2...4)
                .textFieldStyle(.roundedBorder)
            Text("Parameters (JSON Schema)")
                .font(Konjo.mono(10))
                .foregroundStyle(Konjo.fgMute)
            TextEditor(text: $parametersText)
                .font(Konjo.mono(11))
                .frame(height: 120)
                .overlay(RoundedRectangle(cornerRadius: 6).stroke(Konjo.line2, lineWidth: 1))
            if let parseError {
                Text(parseError)
                    .font(Konjo.mono(10))
                    .foregroundStyle(Konjo.err)
            }
            HStack {
                Spacer()
                Button("Cancel") { dismiss() }
                Button("Register") { submit() }
                    .keyboardShortcut(.defaultAction)
                    .disabled(name.trimmingCharacters(in: .whitespaces).isEmpty)
            }
        }
        .padding(20)
        .frame(width: 480)
        .background(Konjo.bg1)
    }

    private func submit() {
        let params: JSONValue
        do {
            params = try JSONDecoder().decode(JSONValue.self, from: Data(parametersText.utf8))
        } catch {
            parseError = "Invalid JSON: \(error.localizedDescription)"
            return
        }
        let body = RegisterToolBody(name: name, description: descriptionText, parameters: params)
        Task {
            await onSubmit(body)
            dismiss()
        }
    }
}
