import SwiftUI

/// The menu-bar companion popover: live status, recent tasks, a quick-submit
/// field, and shortcuts to open the dashboard or quit.
struct MenuBarView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.openWindow) private var openWindow
    @State private var quickGoal = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            statusRow
            Divider()
            quickSubmit
            Divider()
            recentTasks
            Divider()
            footer
        }
        .padding(14)
        .frame(width: 320)
        .background(Konjo.bg)
    }

    private var statusRow: some View {
        HStack {
            ConnectionLED(state: model.connection)
            Spacer()
            Text("\(model.stats.running) running · \(model.stats.queued) queued")
                .font(Konjo.mono(11))
                .foregroundStyle(Konjo.fgDim)
        }
    }

    private var quickSubmit: some View {
        HStack {
            TextField("Quick goal…", text: $quickGoal)
                .konjoField()
                .onSubmit(submit)
            Button(action: submit) {
                Image(systemName: "paperplane.fill")
            }
            .buttonStyle(.borderless)
            .disabled(quickGoal.trimmingCharacters(in: .whitespaces).isEmpty)
        }
    }

    private var recentTasks: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("RECENT")
                .font(Konjo.mono(10))
                .foregroundStyle(Konjo.fgMute)
            if model.tasks.isEmpty {
                Text("No tasks")
                    .font(Konjo.sans(12))
                    .foregroundStyle(Konjo.fgMute)
            } else {
                ForEach(model.tasks.prefix(5)) { task in
                    HStack(spacing: 8) {
                        StatusOrb(status: task.status)
                        Text(task.goal)
                            .font(Konjo.sans(12))
                            .foregroundStyle(Konjo.fg)
                            .lineLimit(1)
                    }
                }
            }
        }
    }

    private var footer: some View {
        HStack {
            Button("Open Dashboard") {
                NSApp.activate(ignoringOtherApps: true)
                openWindow(id: "main")
            }
            .buttonStyle(.borderless)
            Spacer()
            Button("Quit") { NSApp.terminate(nil) }
                .buttonStyle(.borderless)
                .foregroundStyle(Konjo.fgDim)
        }
        .font(Konjo.sans(12))
    }

    private func submit() {
        let goal = quickGoal.trimmingCharacters(in: .whitespaces)
        guard !goal.isEmpty else { return }
        quickGoal = ""
        Task {
            await model.submitTask(CreateTaskBody(
                goal: goal, repo: nil, priority: "normal",
                constraints: nil, allowedDirs: nil, forbiddenDirs: nil, maxRetries: nil
            ))
        }
    }
}
