import SwiftUI

/// A single agent pane, mirroring the web UI's `AgentPane`: a header (status
/// dot + goal + repo), the Forge orb as the centerpiece, and a metrics footer
/// (status · elapsed · short id). Driven by a `TaskSummary` from `/api/tasks`.
struct ForgePane: View {
    let task: TaskSummary
    /// Stream-derived live state (last log line + activity), if any.
    var live: LiveTask?
    var orbSize: CGFloat = 116

    private var isRunning: Bool { task.status.lowercased() == "running" }
    private var phase: String { Konjo.statusPhase(task.status) }

    /// While working the orb glows flame-orange (as the web does); otherwise it
    /// takes the phase color.
    private var orbColor: Color {
        isRunning ? Konjo.flame : Konjo.phaseColor(phase)
    }

    var body: some View {
        VStack(spacing: 0) {
            header
            orbArea
            logTail
            metricsBar
        }
        .background(Konjo.deep.opacity(0.6))
        .overlay(RoundedRectangle(cornerRadius: 10).stroke(borderColor, lineWidth: 1))
        .clipShape(RoundedRectangle(cornerRadius: 10))
    }

    /// Live panes get a faint phase-tinted border, matching the web's
    /// running-pane treatment.
    private var borderColor: Color {
        isRunning ? orbColor.opacity(0.45) : Konjo.line2
    }

    // MARK: Header

    private var header: some View {
        HStack(spacing: 8) {
            Circle()
                .fill(Konjo.statusColor(task.status))
                .frame(width: 8, height: 8)
                .modifier(PulseIf(active: isRunning))
            VStack(alignment: .leading, spacing: 2) {
                Text(task.goal)
                    .font(Konjo.mono(12, weight: .medium))
                    .foregroundStyle(Konjo.paper)
                    .lineLimit(1)
                Text(task.id.prefix(8))
                    .font(Konjo.mono(8))
                    .tracking(1.5)
                    .foregroundStyle(Konjo.paper.opacity(0.4))
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 11)
        .overlay(alignment: .bottom) {
            Rectangle().fill(Konjo.line).frame(height: 1)
        }
    }

    // MARK: Orb

    private var orbArea: some View {
        ForgeOrb(
            phaseColor: orbColor,
            activity: isRunning ? max(0.3, live?.activity ?? 0.6) : 0.18,
            pressure: isRunning ? 0.5 : 0.25,
            size: orbSize,
            running: isRunning
        )
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .padding(.vertical, 14)
    }

    // MARK: Log tail

    @ViewBuilder private var logTail: some View {
        if let line = live?.lastLine, !line.isEmpty {
            Text(line)
                .font(Konjo.mono(8.5))
                .foregroundStyle(logColor)
                .lineLimit(1)
                .truncationMode(.middle)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 14)
                .padding(.vertical, 5)
                .background(Color.black.opacity(0.35))
        }
    }

    private var logColor: Color {
        switch (live?.lastLevel ?? "").lowercased() {
        case "error": return Konjo.rose
        case "warn": return Konjo.flame
        default: return Konjo.paper.opacity(0.55)
        }
    }

    // MARK: Metrics

    private var metricsBar: some View {
        HStack(spacing: 8) {
            Text(task.status.uppercased())
                .foregroundStyle(Konjo.statusColor(task.status))
            Text("·").foregroundStyle(Konjo.paper.opacity(0.25))
            Text(phase).foregroundStyle(orbColor)
            Spacer(minLength: 0)
            Text(TaskTiming.elapsed(from: task.createdAt, to: task.completedAt))
                .foregroundStyle(Konjo.paper.opacity(0.55))
        }
        .font(Konjo.mono(9))
        .tracking(0.5)
        .padding(.horizontal, 14)
        .padding(.vertical, 9)
        .background(Color.black.opacity(0.25))
        .overlay(alignment: .top) {
            Rectangle().fill(Konjo.line).frame(height: 1)
        }
    }
}

/// Applies a gentle opacity/scale pulse while `active`.
private struct PulseIf: ViewModifier {
    let active: Bool
    @State private var on = false

    func body(content: Content) -> some View {
        content
            .opacity(active && on ? 0.55 : 1)
            .scaleEffect(active && on ? 1.3 : 1)
            .animation(active ? .easeInOut(duration: 1).repeatForever(autoreverses: true) : .default, value: on)
            .onAppear { on = active }
            .onChange(of: active) { on = $0 }
    }
}

/// Parses the API's RFC3339 timestamps and renders a compact `mm:ss` elapsed
/// string (created → completed, or created → now for live tasks).
enum TaskTiming {
    private static let parser: ISO8601DateFormatter = {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return f
    }()

    private static let parserNoFraction = ISO8601DateFormatter()

    private static func date(_ iso: String?) -> Date? {
        guard let iso else { return nil }
        return parser.date(from: iso) ?? parserNoFraction.date(from: iso)
    }

    static func elapsed(from created: String?, to completed: String?) -> String {
        guard let start = date(created) else { return "—" }
        let end = date(completed) ?? Date()
        let secs = max(0, Int(end.timeIntervalSince(start)))
        let m = secs / 60, s = secs % 60
        if m > 99 { return "\(m)m" }
        return String(format: "%02d:%02d", m, s)
    }
}
