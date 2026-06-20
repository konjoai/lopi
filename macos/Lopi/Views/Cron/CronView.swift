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
                        Text(CronSpec.describe(schedule.cron))
                            .font(Konjo.mono(11))
                            .foregroundStyle(Konjo.ice)
                            .padding(.horizontal, 8)
                            .padding(.vertical, 2)
                            .background(Konjo.ice.opacity(0.16))
                            .clipShape(Capsule())
                            .help(schedule.cron)
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

/// Create/edit sheet for a schedule. Drives a friendly frequency picker
/// (Hourly/Daily/Weekly/Monthly) that generates the cron for you, with a raw
/// Custom escape hatch and a live human-readable + cron preview.
struct ScheduleEditor: View {
    @Environment(\.dismiss) private var dismiss
    let schedule: Schedule?
    let onSave: (ScheduleBody) async -> Void

    @State private var name = ""
    @State private var spec = CronSpec()
    @State private var goal = ""
    @State private var repo = ""
    @State private var priority = "normal"

    private let priorities = ["low", "normal", "high", "critical"]

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text(schedule == nil ? "New Schedule" : "Edit Schedule")
                .font(Konjo.sans(18, weight: .semibold))
                .foregroundStyle(Konjo.fg)

            field("Name") { TextField("e.g. Nightly dependency audit", text: $name).konjoField() }

            field("Frequency") {
                VStack(alignment: .leading, spacing: 10) {
                    Picker("", selection: $spec.frequency) {
                        ForEach(CronFrequency.allCases) { Text($0.label).tag($0) }
                    }
                    .labelsHidden()
                    .pickerStyle(.segmented)
                    frequencyControls
                }
            }

            schedulePreview

            field("Goal") {
                TextField("What should the agent do?", text: $goal, axis: .vertical)
                    .lineLimit(2...5).konjoField()
            }
            field("Repo path (optional)") {
                TextField("./path or owner/name", text: $repo).konjoField()
            }
            field("Priority") {
                Picker("", selection: $priority) {
                    ForEach(priorities, id: \.self) { Text($0.capitalized).tag($0) }
                }
                .labelsHidden().pickerStyle(.segmented)
            }

            HStack {
                Spacer()
                Button("Cancel") { dismiss() }.konjoButton()
                Button("Save") { save() }
                    .konjoButton(Konjo.ice, prominent: true)
                    .keyboardShortcut(.defaultAction)
                    .disabled(name.isEmpty || goal.isEmpty)
            }
            .padding(.top, 4)
        }
        .padding(22)
        .frame(width: 500)
        .background(Konjo.bg1)
        .onAppear(perform: prime)
    }

    /// The controls that change per frequency.
    @ViewBuilder private var frequencyControls: some View {
        switch spec.frequency {
        case .hourly:
            HStack(spacing: 8) {
                Text("at minute").font(Konjo.mono(11)).foregroundStyle(Konjo.fgDim)
                minutePicker
            }
        case .daily:
            timePicker
        case .weekly:
            HStack(spacing: 10) {
                Picker("", selection: $spec.weekday) {
                    ForEach(0..<7, id: \.self) { Text(CronSpec.weekdayNames[$0]).tag($0) }
                }
                .labelsHidden().frame(width: 130)
                timePicker
            }
        case .monthly:
            HStack(spacing: 10) {
                HStack(spacing: 6) {
                    Text("day").font(Konjo.mono(11)).foregroundStyle(Konjo.fgDim)
                    Picker("", selection: $spec.dayOfMonth) {
                        ForEach(1...31, id: \.self) { Text("\($0)").tag($0) }
                    }
                    .labelsHidden().frame(width: 70)
                }
                timePicker
            }
        case .custom:
            TextField("min hour dom mon dow — e.g. 0 2 * * *", text: $spec.custom).konjoField()
        }
    }

    private var timePicker: some View {
        HStack(spacing: 6) {
            Text("at").font(Konjo.mono(11)).foregroundStyle(Konjo.fgDim)
            Picker("", selection: $spec.hour) {
                ForEach(0..<24, id: \.self) { Text(String(format: "%02d", $0)).tag($0) }
            }
            .labelsHidden().frame(width: 66)
            Text(":").foregroundStyle(Konjo.fgDim)
            minutePicker
        }
    }

    private var minutePicker: some View {
        Picker("", selection: $spec.minute) {
            ForEach(0..<60, id: \.self) { Text(String(format: "%02d", $0)).tag($0) }
        }
        .labelsHidden().frame(width: 66)
    }

    /// Live human-readable summary + the generated cron expression.
    private var schedulePreview: some View {
        HStack(spacing: 10) {
            Image(systemName: "clock.arrow.circlepath").foregroundStyle(Konjo.ice)
            VStack(alignment: .leading, spacing: 2) {
                Text(spec.summary).font(Konjo.sans(12, weight: .medium)).foregroundStyle(Konjo.fg)
                Text(spec.cron).font(Konjo.mono(10)).foregroundStyle(Konjo.fgMute)
            }
            Spacer()
        }
        .padding(.horizontal, 12).padding(.vertical, 9)
        .background(RoundedRectangle(cornerRadius: 8).fill(Konjo.ice.opacity(0.08)))
        .overlay(RoundedRectangle(cornerRadius: 8).stroke(Konjo.ice.opacity(0.2), lineWidth: 1))
    }

    private func field<Content: View>(_ label: String, @ViewBuilder _ content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 5) {
            Text(label.uppercased())
                .font(Konjo.mono(9, weight: .semibold)).tracking(1.2).foregroundStyle(Konjo.fgMute)
            content()
        }
    }

    private func prime() {
        guard let schedule else { return }
        name = schedule.name
        spec = CronSpec.parse(schedule.cron)
        goal = schedule.goal
        repo = schedule.repo ?? ""
        priority = schedule.priority
    }

    private func save() {
        let body = ScheduleBody(
            name: name,
            cron: spec.cron,
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
