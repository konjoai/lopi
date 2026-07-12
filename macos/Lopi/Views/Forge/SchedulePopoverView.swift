import SwiftUI

/// SchedulePopover — the cyan schedule button's content. `cron.raw` is WIRED at
/// loop scope (mirrors `ScheduleEntry.cron`); the preset fields two-way-sync with
/// it. Generalized to a `scheduled`/`cron` value pair + callbacks, so the same
/// view mounts scoped to one loop or (STUBBED) to the whole stack.
struct SchedulePopoverView: View {
    var scheduled: Bool
    var cron: CronConfig
    var onToggle: () -> Void
    var onChange: (CronConfig) -> Void

    private let freqs: [CronFreq] = [.everyMinute, .hourly, .daily, .weekly, .custom]

    /// Patch the cron and re-derive `raw` unless the operator is on `custom`.
    private func patch(_ mutate: (inout CronConfig) -> Void) {
        var next = cron
        mutate(&next)
        if next.freq != .custom { next.raw = buildCronString(next) }
        onChange(next)
    }

    var body: some View {
        PopoverChrome(systemImage: "clock", title: "schedule", accent: Konjo.ice) {
            VStack(alignment: .leading, spacing: 11) {
                HStack(spacing: 9) {
                    StackToggle(isOn: scheduled, accent: Konjo.ice, onToggle: onToggle)
                    Text("run on a schedule").font(Konjo.mono(11)).foregroundStyle(Konjo.fg)
                }
                if scheduled {
                    freqRow
                    detailRow
                    rawRow
                    Text("\(cronHuman(cron)) → \(buildCronString(cron))")
                        .font(Konjo.mono(9)).foregroundStyle(Konjo.fgDim)
                    nextRuns
                }
            }
        }
    }

    private var freqRow: some View {
        HStack(spacing: 5) {
            ForEach(freqs, id: \.self) { f in
                Button { patch { $0.freq = f } } label: {
                    Text(f.rawValue).font(Konjo.mono(10))
                        .foregroundStyle(cron.freq == f ? Konjo.ice : Konjo.fgDim)
                        .padding(.horizontal, 10).padding(.vertical, 4)
                        .background(cron.freq == f ? Konjo.ice.opacity(0.14) : Color.clear)
                        .overlay(Capsule().stroke(cron.freq == f ? Konjo.ice.opacity(0.45) : Konjo.line, lineWidth: 1))
                        .clipShape(Capsule())
                }
                .buttonStyle(.plain)
            }
        }
    }

    @ViewBuilder private var detailRow: some View {
        switch cron.freq {
        case .weekly:
            HStack(spacing: 7) {
                Text("on").font(Konjo.mono(10)).foregroundStyle(Konjo.fgDim)
                dowMenu
                Text("at").font(Konjo.mono(10)).foregroundStyle(Konjo.fgDim)
                timePickers
            }
        case .daily:
            HStack(spacing: 7) {
                Text("at").font(Konjo.mono(10)).foregroundStyle(Konjo.fgDim)
                timePickers
            }
        case .hourly:
            HStack(spacing: 7) {
                Text("at minute").font(Konjo.mono(10)).foregroundStyle(Konjo.fgDim)
                StackCombo(value: cron.min, range: 0...59) { m in patch { $0.min = m } }
            }
        default:
            EmptyView()
        }
    }

    private var timePickers: some View {
        HStack(spacing: 5) {
            StackCombo(value: cron.hour12, range: 1...12) { h in patch { $0.hour12 = h } }
            Text(":").font(Konjo.mono(11, weight: .bold)).foregroundStyle(Konjo.fgDim)
            StackCombo(value: cron.min, range: 0...59) { m in patch { $0.min = m } }
            StackSegmented(options: [(CronConfig.AmPm.AM, "AM"), (.PM, "PM")], selected: cron.ampm, accent: Konjo.ice) { ap in
                patch { $0.ampm = ap }
            }
        }
    }

    private var dowMenu: some View {
        Menu {
            ForEach(Dow.allCases, id: \.self) { d in
                Button(d.rawValue) { patch { $0.dow = d } }
            }
        } label: {
            Text(cron.dow.rawValue).font(Konjo.mono(10)).foregroundStyle(Konjo.fg)
        }
        .menuStyle(.borderlessButton).fixedSize()
    }

    private var rawRow: some View {
        HStack(spacing: 8) {
            Text("CRON").font(Konjo.mono(8)).tracking(0.6).foregroundStyle(Konjo.fgDim)
            TextField("* * * * *", text: Binding(
                get: { buildCronString(cron) },
                set: { raw in onChange({ var c = cron; c.freq = .custom; c.raw = raw; return c }()) }))
                .textFieldStyle(.plain).font(Konjo.mono(10.5)).foregroundStyle(Konjo.ice)
                .padding(5).background(Color.white.opacity(0.03))
                .overlay(RoundedRectangle(cornerRadius: 5).stroke(Konjo.line, lineWidth: 1))
        }
    }

    @ViewBuilder private var nextRuns: some View {
        let runs = computeNextRuns(buildCronString(cron), from: Date(), count: 3)
        if !runs.isEmpty {
            VStack(alignment: .leading, spacing: 3) {
                Text("next runs:").font(Konjo.mono(9)).foregroundStyle(Konjo.fgDim)
                ForEach(Array(runs.enumerated()), id: \.offset) { _, r in
                    Text("– \(Self.fmt.string(from: r))").font(Konjo.mono(9)).foregroundStyle(Konjo.fgMute)
                }
            }
        }
    }

    private static let fmt: DateFormatter = {
        let f = DateFormatter()
        f.dateFormat = "MMM d, h:mm a"
        return f
    }()
}
