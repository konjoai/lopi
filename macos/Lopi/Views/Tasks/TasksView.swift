import SwiftUI

/// Task list + detail. The detail pane backfills logs from
/// `/api/tasks/:id/logs`; the New Task composer posts to `/api/tasks`.
struct TasksView: View {
    @Environment(AppModel.self) private var model
    @State private var selected: TaskSummary?
    @State private var showComposer = false

    var body: some View {
        HSplitView {
            list
                .frame(minWidth: 280)
            detail
                .frame(minWidth: 360)
        }
        .background(Konjo.bg)
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button {
                    showComposer = true
                } label: {
                    Label("New Task", systemImage: "plus")
                }
            }
        }
        .sheet(isPresented: $showComposer) {
            NewTaskSheet { body in
                await model.submitTask(body)
            }
        }
    }

    private var list: some View {
        List(model.tasks, selection: $selected) { task in
            HStack(spacing: 10) {
                StatusOrb(status: task.status)
                VStack(alignment: .leading, spacing: 2) {
                    Text(task.goal)
                        .font(Konjo.sans(13))
                        .foregroundStyle(Konjo.fg)
                        .lineLimit(1)
                    Text(task.status)
                        .font(Konjo.mono(10))
                        .foregroundStyle(Konjo.fgMute)
                }
            }
            .tag(task)
        }
        .refreshable { await model.refreshTasks() }
    }

    @ViewBuilder private var detail: some View {
        if let task = selected {
            TaskDetailView(task: task)
                .id(task.id)
        } else {
            VStack {
                Text("Select a task")
                    .font(Konjo.sans(14))
                    .foregroundStyle(Konjo.fgMute)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }
}

/// Detail pane: metadata, actions, and a backfilled log tail.
struct TaskDetailView: View {
    @Environment(AppModel.self) private var model
    let task: TaskSummary
    @State private var logs: [TaskLog] = []

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                header
                actions
                logSection
            }
            .padding(20)
        }
        .task { logs = await model.logs(for: task.id) }
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                StatusOrb(status: task.status)
                Text(task.status)
                    .font(Konjo.mono(12))
                    .foregroundStyle(Konjo.fgDim)
                Spacer()
                Text(task.id)
                    .font(Konjo.mono(10))
                    .foregroundStyle(Konjo.fgMute)
            }
            Text(task.goal)
                .font(Konjo.sans(16, weight: .medium))
                .foregroundStyle(Konjo.fg)
        }
    }

    private var actions: some View {
        HStack {
            Button(role: .destructive) {
                Task { await model.cancelTask(task.id) }
            } label: {
                Label("Cancel", systemImage: "xmark.circle")
            }
            Button {
                Task { logs = await model.logs(for: task.id) }
            } label: {
                Label("Refresh logs", systemImage: "arrow.clockwise")
            }
        }
    }

    private var logSection: some View {
        KonjoPanel {
            VStack(alignment: .leading, spacing: 6) {
                Text("LOGS")
                    .font(Konjo.mono(11))
                    .foregroundStyle(Konjo.fgMute)
                if logs.isEmpty {
                    Text("No logs recorded")
                        .font(Konjo.mono(11))
                        .foregroundStyle(Konjo.fgMute)
                } else {
                    ForEach(logs) { log in
                        Text(log.line)
                            .font(Konjo.mono(11))
                            .foregroundStyle(color(for: log.level))
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
            }
        }
    }

    private func color(for level: String) -> Color {
        switch level.lowercased() {
        case "error": return Konjo.err
        case "warn": return Konjo.warn
        case "debug": return Konjo.fgMute
        default: return Konjo.fgDim
        }
    }
}

/// New Task composer matching the web UI's form fields.
struct NewTaskSheet: View {
    @Environment(\.dismiss) private var dismiss
    let onSubmit: (CreateTaskBody) async -> Void

    @State private var goal = ""
    @State private var repo = ""
    @State private var priority = "normal"
    @State private var submitting = false

    private let priorities = ["low", "normal", "high", "critical"]

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("New Task")
                .font(Konjo.sans(18, weight: .semibold))
                .foregroundStyle(Konjo.fg)

            TextField("Goal", text: $goal, axis: .vertical)
                .lineLimit(3...6)
                .konjoField()

            TextField("Repo path (optional)", text: $repo)
                .konjoField()

            Picker("Priority", selection: $priority) {
                ForEach(priorities, id: \.self) { Text($0.capitalized).tag($0) }
            }
            .pickerStyle(.segmented)

            HStack {
                Spacer()
                Button("Cancel") { dismiss() }
                Button("Submit") { submit() }
                    .keyboardShortcut(.defaultAction)
                    .disabled(goal.trimmingCharacters(in: .whitespaces).isEmpty || submitting)
            }
        }
        .padding(20)
        .frame(width: 460)
        .background(Konjo.bg1)
    }

    private func submit() {
        submitting = true
        let body = CreateTaskBody(
            goal: goal,
            repo: repo.isEmpty ? nil : repo,
            priority: priority,
            constraints: nil,
            allowedDirs: nil,
            forbiddenDirs: nil,
            maxRetries: nil
        )
        Task {
            await onSubmit(body)
            dismiss()
        }
    }
}
