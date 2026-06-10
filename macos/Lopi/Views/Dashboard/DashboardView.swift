import SwiftUI

/// The "Forge" view — a parity port of the web dashboard's `AgentGrid`. A grid
/// of agent panes (each a live Forge orb) over the ambient starfield, with a
/// stats header, a quick-submit composer, and a live log-stream panel.
struct DashboardView: View {
    @EnvironmentObject private var model: AppModel
    @State private var selected: TaskSummary?
    @State private var showComposer = false

    private let columns = [GridItem(.adaptive(minimum: 240, maximum: 360), spacing: 14)]

    var body: some View {
        ZStack {
            KonjoBackground()
            if model.tasks.isEmpty {
                emptyState
            } else {
                content
            }
        }
        .refreshable { await model.refreshAll() }
        .sheet(item: $selected) { task in
            TaskDetailSheet(task: task)
        }
        .sheet(isPresented: $showComposer) {
            NewTaskSheet { body in await model.submitTask(body) }
        }
    }

    private var content: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                header
                grid
                if !model.recentLogs.isEmpty { logStream }
            }
            .padding(18)
        }
    }

    // MARK: Header

    private var header: some View {
        HStack(spacing: 18) {
            stat("\(model.stats.running)", "running", Konjo.jade)
            stat("\(model.stats.queued)", "queued", Konjo.sun)
            stat("\(model.stats.succeeded)", "done", Konjo.jade.opacity(0.7))
            stat("\(model.stats.failed)", "failed", Konjo.rose)
            Spacer(minLength: 0)
            newTaskButton
        }
        .padding(.horizontal, 4)
    }

    private func stat(_ value: String, _ label: String, _ accent: Color) -> some View {
        HStack(spacing: 7) {
            Text(value)
                .font(Konjo.sans(18, weight: .semibold))
                .foregroundStyle(accent)
                .monospacedDigit()
            Text(label.uppercased())
                .font(Konjo.mono(9)).tracking(1.5)
                .foregroundStyle(Konjo.fgMute)
        }
    }

    private var newTaskButton: some View {
        Button { showComposer = true } label: {
            Label("New Task", systemImage: "plus")
                .font(Konjo.mono(11, weight: .medium))
        }
        .buttonStyle(.plain)
        .foregroundStyle(Konjo.ice)
        .padding(.horizontal, 12).padding(.vertical, 6)
        .overlay(Capsule().stroke(Konjo.ice.opacity(0.5), lineWidth: 1))
    }

    // MARK: Grid

    private var grid: some View {
        LazyVGrid(columns: columns, spacing: 14) {
            ForEach(model.tasks.prefix(12)) { task in
                Button { selected = task } label: {
                    ForgePane(task: task, live: model.live[task.id])
                        .frame(height: 226)
                }
                .buttonStyle(.plain)
            }
        }
    }

    // MARK: Live log stream

    private var logStream: some View {
        KonjoPanel {
            VStack(alignment: .leading, spacing: 5) {
                Text("LOG STREAM")
                    .font(Konjo.mono(10)).tracking(2)
                    .foregroundStyle(Konjo.fgMute)
                ForEach(Array(model.recentLogs.suffix(10).enumerated()), id: \.offset) { _, line in
                    Text(line)
                        .font(Konjo.mono(10))
                        .foregroundStyle(Konjo.paper.opacity(0.6))
                        .lineLimit(1).truncationMode(.middle)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    // MARK: Empty

    private var emptyState: some View {
        VStack(spacing: 10) {
            ForgeOrb(phaseColor: Konjo.ice, activity: 0.25, pressure: 0.3, size: 128)
            Text("no agents")
                .font(Konjo.sans(20, weight: .bold))
                .foregroundStyle(Konjo.paper.opacity(0.35))
            Button { showComposer = true } label: {
                Text("submit a goal to start a run")
                    .font(Konjo.mono(10)).tracking(2)
                    .foregroundStyle(Konjo.ice.opacity(0.8))
            }
            .buttonStyle(.plain)
        }
        .sheet(isPresented: $showComposer) {
            NewTaskSheet { body in await model.submitTask(body) }
        }
    }
}

/// Wraps the shared `TaskDetailView` in a dismissible sheet chrome.
private struct TaskDetailSheet: View {
    @Environment(\.dismiss) private var dismiss
    let task: TaskSummary

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text(task.id.prefix(8))
                    .font(Konjo.mono(11)).foregroundStyle(Konjo.fgMute)
                Spacer()
                Button("Done") { dismiss() }
                    .keyboardShortcut(.cancelAction)
            }
            .padding(14)
            Divider().overlay(Konjo.line)
            TaskDetailView(task: task)
        }
        .frame(width: 560, height: 560)
        .background(Konjo.black)
    }
}
