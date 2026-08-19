import SwiftUI

/// Standalone cron scheduling — the iOS counterpart of macOS's `CronView.swift`
/// (`/schedules` on web). Distinct from the per-stack/per-card schedule
/// *popover* (`ScheduleFacetView` in `FacetPopovers.swift`, backed by
/// `LopiStacksKit`'s `CronConfig`) — this is standalone goal-on-a-cron entries
/// backed by `/api/schedules`, reusing `AppModel`'s already-working
/// `refreshSchedules`/`saveSchedule`/`toggleSchedule`/`runScheduleNow`/
/// `deleteSchedule` (already exercised from iOS by nothing yet, but wired
/// identically to macOS — no new networking, per the parity plan's port-cost
/// estimate). Presented as a sheet from `StackOverviewScreen`, same pattern as
/// `BudgetScreen`/`ServerConfigScreen`.
struct SchedulingScreen: View {
    @Environment(AppModel.self) private var model
    @State private var editing: Schedule?
    @State private var showCreate = false

    var body: some View {
        NavigationStack {
            Group {
                if model.schedules.isEmpty {
                    Text("No schedules yet — create one to run a goal on a cron.")
                        .font(Konjo.sans(13))
                        .foregroundStyle(Konjo.fgMute)
                        .padding(.top, 40)
                        .frame(maxWidth: .infinity)
                } else {
                    List {
                        ForEach(model.schedules) { schedule in
                            row(schedule)
                                .listRowInsets(EdgeInsets(top: 6, leading: 16, bottom: 6, trailing: 16))
                                .listRowBackground(Konjo.panel)
                                .listRowSeparator(.hidden)
                                .swipeActions(edge: .trailing, allowsFullSwipe: true) {
                                    Button(role: .destructive) {
                                        Haptics.warning()
                                        Task { await model.deleteSchedule(schedule) }
                                    } label: {
                                        Label("Delete", systemImage: "trash.fill")
                                    }
                                    .tint(Konjo.rose)
                                    Button {
                                        Haptics.impact()
                                        Task { await model.runScheduleNow(schedule) }
                                    } label: {
                                        Label("Run now", systemImage: "play.fill")
                                    }
                                    .tint(Konjo.ice)
                                }
                                .contentShape(Rectangle())
                                .onTapGesture { Haptics.tap(); editing = schedule }
                        }
                    }
                    .listStyle(.plain)
                    .scrollContentBackground(.hidden)
                    .refreshable { await model.refreshSchedules() }
                }
            }
            .background(Konjo.panel)
            .navigationTitle("scheduling")
            .toolbar {
                ToolbarItem(placement: .primaryAction) {
                    Button {
                        Haptics.tap()
                        showCreate = true
                    } label: {
                        Image(systemName: "plus")
                    }
                }
            }
        }
        .task { await model.refreshSchedules() }
        .sheet(isPresented: $showCreate) {
            ScheduleEditorScreen(schedule: nil) { body in
                await model.saveSchedule(id: nil, body)
            }
        }
        .sheet(item: $editing) { schedule in
            ScheduleEditorScreen(schedule: schedule) { body in
                await model.saveSchedule(id: schedule.id, body)
            }
        }
    }

    private func row(_ schedule: Schedule) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 8) {
                Text(schedule.name)
                    .font(Konjo.sans(14, weight: .semibold))
                    .foregroundStyle(Konjo.fg)
                Text(CronSpec.describe(schedule.cron))
                    .font(Konjo.mono(10))
                    .foregroundStyle(Konjo.ice)
                    .padding(.horizontal, 7).padding(.vertical, 2)
                    .background(Konjo.ice.opacity(0.16), in: Capsule())
                Spacer()
                Toggle("", isOn: Binding(
                    get: { schedule.enabled },
                    set: { _ in Haptics.selection(); Task { await model.toggleSchedule(schedule) } }
                ))
                .labelsHidden()
                .toggleStyle(.switch)
                .tint(Konjo.flame)
            }
            Text(schedule.goal)
                .font(Konjo.sans(12))
                .foregroundStyle(Konjo.fgDim)
                .lineLimit(2)
            HStack(spacing: 14) {
                Label(nextRunText(schedule), systemImage: "clock")
                Label(lastRunText(schedule), systemImage: "checkmark.circle")
            }
            .font(Konjo.mono(9.5))
            .foregroundStyle(Konjo.fgMute)
        }
        .padding(.vertical, 4)
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

/// Create/edit sheet — a friendly frequency picker (Hourly/Daily/Weekly/
/// Monthly) that generates the cron for you, with a raw Custom escape hatch
/// and a live human-readable + cron preview. iOS layout: segmented/menu
/// pickers instead of macOS's fixed-width inline pickers, otherwise the same
/// `CronSpec` logic (now shared via `Lopi/Store/CronPresets.swift`).
struct ScheduleEditorScreen: View {
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
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    field("Name") {
                        styledField("e.g. Nightly dependency audit", text: $name)
                    }

                    field("Frequency") {
                        VStack(alignment: .leading, spacing: 10) {
                            Picker("", selection: Binding(
                                get: { spec.frequency },
                                set: { Haptics.selection(); spec.frequency = $0 }
                            )) {
                                ForEach(CronFrequency.allCases) { Text($0.label).tag($0) }
                            }
                            .labelsHidden()
                            .pickerStyle(.segmented)
                            frequencyControls
                        }
                    }

                    schedulePreview

                    field("Goal") {
                        styledField("What should the agent do?", text: $goal, axis: .vertical, lines: 2...5)
                    }
                    field("Repo path (optional)") {
                        styledField("./path or owner/name", text: $repo)
                    }
                    field("Priority") {
                        Picker("", selection: Binding(
                            get: { priority },
                            set: { Haptics.selection(); priority = $0 }
                        )) {
                            ForEach(priorities, id: \.self) { Text($0.capitalized).tag($0) }
                        }
                        .labelsHidden().pickerStyle(.segmented)
                    }
                }
                .padding(16)
            }
            .background(Konjo.panel)
            .navigationTitle(schedule == nil ? "new schedule" : "edit schedule")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { Haptics.tap(); dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") { Haptics.impact(); save() }
                        .disabled(name.isEmpty || goal.isEmpty)
                }
            }
        }
        .onAppear(perform: prime)
    }

    /// Matches the composer/config-screen text field chrome used across the
    /// rest of the iOS app (`StackDetailScreen`/`ServerConfigScreen`).
    private func styledField(
        _ placeholder: String, text: Binding<String>, axis: Axis = .horizontal, lines: ClosedRange<Int>? = nil
    ) -> some View {
        Group {
            if axis == .vertical {
                TextField(placeholder, text: text, axis: .vertical)
                    .lineLimit(lines ?? 1...1)
            } else {
                TextField(placeholder, text: text)
            }
        }
        .font(Konjo.sans(13))
        .foregroundStyle(Konjo.fg)
        .padding(9)
        .background(Color.white.opacity(0.02))
        .clipShape(RoundedRectangle(cornerRadius: 7))
        .overlay(RoundedRectangle(cornerRadius: 7).stroke(Konjo.line2, lineWidth: 1))
    }

    @ViewBuilder private var frequencyControls: some View {
        switch spec.frequency {
        case .hourly:
            HStack(spacing: 8) {
                Text("at minute").font(Konjo.mono(11)).foregroundStyle(Konjo.fgDim)
                Stepper(String(format: "%02d", spec.minute), value: Binding(
                    get: { spec.minute }, set: { Haptics.selection(); spec.minute = $0 }
                ), in: 0...59).labelsHidden()
                Text(String(format: ":%02d", spec.minute)).font(Konjo.mono(11)).foregroundStyle(Konjo.fg)
            }
        case .daily:
            timeRow
        case .weekly:
            HStack(spacing: 10) {
                Menu {
                    ForEach(0..<7, id: \.self) { i in
                        Button(CronSpec.weekdayNames[i]) { Haptics.selection(); spec.weekday = i }
                    }
                } label: {
                    menuLabel(CronSpec.weekdayNames[max(0, min(6, spec.weekday))])
                }
                timeRow
            }
        case .monthly:
            HStack(spacing: 10) {
                HStack(spacing: 6) {
                    Text("day").font(Konjo.mono(11)).foregroundStyle(Konjo.fgDim)
                    Menu {
                        ForEach(1...31, id: \.self) { d in
                            Button("\(d)") { Haptics.selection(); spec.dayOfMonth = d }
                        }
                    } label: {
                        menuLabel("\(spec.dayOfMonth)")
                    }
                }
                timeRow
            }
        case .custom:
            styledField("min hour dom mon dow — e.g. 0 2 * * *", text: $spec.custom)
        }
    }

    private var timeRow: some View {
        HStack(spacing: 6) {
            Text("at").font(Konjo.mono(11)).foregroundStyle(Konjo.fgDim)
            Menu {
                ForEach(0..<24, id: \.self) { h in
                    Button(String(format: "%02d", h)) { Haptics.selection(); spec.hour = h }
                }
            } label: {
                menuLabel(String(format: "%02d", spec.hour))
            }
            Text(":").foregroundStyle(Konjo.fgDim)
            Menu {
                ForEach(0..<60, id: \.self) { m in
                    Button(String(format: "%02d", m)) { Haptics.selection(); spec.minute = m }
                }
            } label: {
                menuLabel(String(format: "%02d", spec.minute))
            }
        }
    }

    private func menuLabel(_ text: String) -> some View {
        HStack(spacing: 4) {
            Text(text).font(Konjo.mono(11)).foregroundStyle(Konjo.fg)
            Image(systemName: "chevron.down").font(.system(size: 8)).foregroundStyle(Konjo.fgMute)
        }
        .padding(.horizontal, 8).padding(.vertical, 4)
        .overlay(RoundedRectangle(cornerRadius: 6).stroke(Konjo.line2, lineWidth: 1))
    }

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
