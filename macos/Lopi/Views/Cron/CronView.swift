import SwiftUI

/// OpenClaw-style cron screen: list of schedules with next/last run, enable
/// toggle, run-now, edit, delete — backed by `/api/schedules`.
struct CronView: View {
    @Environment(AppModel.self) private var model
    @State private var editing: Schedule?
    @State private var showCreate = false

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                if model.schedules.isEmpty {
                    Text("No schedules yet — create one to run a goal on a cron.")
                        .font(Konjo.sans(13))
                        .foregroundStyle(Konjo.fgMute)
                        .padding(.top, 40)
                } else {
                    ForEach(model.schedules) { schedule in
                        row(schedule)
                    }
                }
            }
            .padding(20)
        }
        .background(Konjo.bg)
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button { showCreate = true } label: {
                    Label("New Schedule", systemImage: "plus")
                }
            }
        }
        .refreshable { await model.refreshSchedules() }
        .sheet(isPresented: $showCreate) {
            ScheduleEditor(schedule: nil) { body in
                await model.saveSchedule(id: nil, body)
            }
        }
        .sheet(item: $editing) { schedule in
            ScheduleEditor(schedule: schedule) { body in
                await model.saveSchedule(id: schedule.id, body)
            }
        }
    }

    private func row(_ schedule: Schedule) -> some View {
        KonjoPanel {
            HStack(alignment: .top, spacing: 16) {
                VStack(alignment: .leading, spacing: 6) {
                    HStack(spacing: 8) {
                        Text(schedule.name)
                            .font(Konjo.sans(15, weight: .semibold))
                            .foregroundStyle(Konjo.fg)
                        Text(schedule.cron)
                            .font(Konjo.mono(11))
                            .foregroundStyle(Konjo.konjo2)
                            .padding(.horizontal, 6)
                            .padding(.vertical, 2)
                            .background(Konjo.konjo.opacity(0.18))
                            .clipShape(Capsule())
                    }
                    Text(schedule.goal)
                        .font(Konjo.sans(12))
                        .foregroundStyle(Konjo.fgDim)
                        .lineLimit(2)
                    HStack(spacing: 14) {
                        Label(nextRunText(schedule), systemImage: "clock")
                        Label(lastRunText(schedule), systemImage: "checkmark.circle")
                    }
                    .font(Konjo.mono(10))
                    .foregroundStyle(Konjo.fgMute)
                }
                Spacer()
                controls(schedule)
            }
        }
    }

    private func controls(_ schedule: Schedule) -> some View {
        VStack(alignment: .trailing, spacing: 8) {
            Toggle("", isOn: Binding(
                get: { schedule.enabled },
                set: { _ in Task { await model.toggleSchedule(schedule) } }
            ))
            .labelsHidden()
            .toggleStyle(.switch)
            .tint(Konjo.konjo)

            HStack(spacing: 10) {
                Button { Task { await model.runScheduleNow(schedule) } } label: {
                    Image(systemName: "play.fill")
                }
                .help("Run now")
                Button { editing = schedule } label: {
                    Image(systemName: "pencil")
                }
                .help("Edit")
                Button(role: .destructive) { Task { await model.deleteSchedule(schedule) } } label: {
                    Image(systemName: "trash")
                }
                .help("Delete")
            }
            .buttonStyle(.borderless)
            .foregroundStyle(Konjo.fgDim)
        }
    }

    private func nextRunText(_ schedule: Schedule) -> String {
        guard let next = schedule.nextRuns?.first else { return "next: —" }
        return "next: \(DateFormatting.short(next))"
    }

    private func lastRunText(_ schedule: Schedule) -> String {
        guard let last = schedule.lastRun else { return "last: never" }
        return "last: \(last.outcome) @ \(DateFormatting.short(last.firedAt))"
    }
}

/// Create/edit sheet for a schedule, with a live "next runs" preview.
struct ScheduleEditor: View {
    @Environment(\.dismiss) private var dismiss
    let schedule: Schedule?
    let onSave: (ScheduleBody) async -> Void

    @State private var name = ""
    @State private var cron = "0 2 * * *"
    @State private var goal = ""
    @State private var repo = ""
    @State private var priority = "normal"

    private let priorities = ["low", "normal", "high", "critical"]

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(schedule == nil ? "New Schedule" : "Edit Schedule")
                .font(Konjo.sans(18, weight: .semibold))
                .foregroundStyle(Konjo.fg)

            TextField("Name", text: $name)
                .konjoField()
            HStack {
                TextField("Cron (5-field, e.g. 0 2 * * *)", text: $cron)
                    .konjoField()
            }
            previewLine
            TextField("Goal", text: $goal, axis: .vertical)
                .lineLimit(2...5)
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
                Button("Save") { save() }
                    .keyboardShortcut(.defaultAction)
                    .disabled(name.isEmpty || goal.isEmpty)
            }
        }
        .padding(20)
        .frame(width: 480)
        .background(Konjo.bg1)
        .onAppear(perform: prime)
    }

    /// A rough local cron sanity hint (server is the authority on validity).
    private var previewLine: some View {
        let fields = cron.split(separator: " ").count
        let ok = fields == 5
        return Text(ok ? "Looks like a 5-field cron expression" : "Expected 5 fields (min hour dom month dow)")
            .font(Konjo.mono(10))
            .foregroundStyle(ok ? Konjo.ok : Konjo.warn)
    }

    private func prime() {
        guard let schedule else { return }
        name = schedule.name
        cron = schedule.cron
        goal = schedule.goal
        repo = schedule.repo ?? ""
        priority = schedule.priority
    }

    private func save() {
        let body = ScheduleBody(
            name: name,
            cron: cron,
            goal: goal,
            repo: repo.isEmpty ? nil : repo,
            priority: priority,
            allowedDirs: nil,
            forbiddenDirs: nil,
            enabled: schedule?.enabled ?? true
        )
        Task {
            await onSave(body)
            dismiss()
        }
    }
}

/// Trims ISO-8601 timestamps to a compact display form.
enum DateFormatting {
    static func short(_ iso: String) -> String {
        // Show "MM-dd HH:mm" from an RFC3339 string without heavy parsing.
        guard iso.count >= 16 else { return iso }
        let datePart = iso.prefix(10).suffix(5) // MM-dd
        let timePart = iso.dropFirst(11).prefix(5) // HH:mm
        return "\(datePart) \(timePart)"
    }
}
