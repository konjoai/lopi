import SwiftUI
import LopiStacksKit

/// MaxxPopover — the bolt-icon button's content. The enable toggle is wired to
/// real `/api/maxx` CRUD (create-on-first-enable → then enable/disable),
/// unlike `SchedulePopoverView` which stays client-local until stack submit.
/// Quiet-hours and headroom-gate are fixed, read-only policy text in this
/// sprint — the only interactive control is the enable toggle. Mirrors web's
/// `MaxxPopover.svelte`.
struct MaxxPopoverView: View {
    @Environment(AppModel.self) private var model

    var maxx: MaxxConfig
    var entryId: String?
    var goal: String
    var repo: String?
    /// Called after a toggle's CRUD call settles — patches the card with the
    /// new `enabled` state and (on first enable) the freshly created entry id.
    var onToggled: (_ enabled: Bool, _ entryId: String?) -> Void

    @State private var busy = false
    @State private var error: String?
    @State private var quota: QuotaSnapshot?
    @State private var quotaError: String?

    var body: some View {
        PopoverChrome(systemImage: "bolt.fill", title: "MAXX", accent: FacetAccent.maxx, width: 320) {
            VStack(alignment: .leading, spacing: 11) {
                HStack(spacing: 9) {
                    StackToggle(isOn: maxx.enabled, accent: FacetAccent.maxx, onToggle: toggle)
                        .disabled(busy)
                        .accessibilityIdentifier("stack.maxxToggle")
                    Text("enable MAXX").font(Konjo.mono(11)).foregroundStyle(Konjo.fg)
                }
                if let error {
                    Text(error).font(Konjo.mono(10)).foregroundStyle(Konjo.rose)
                }

                Text("run").font(Konjo.mono(8)).tracking(0.6).foregroundStyle(Konjo.fgDim)
                VStack(alignment: .leading, spacing: 4) {
                    HStack(spacing: 4) {
                        Text("After hours").font(Konjo.mono(10.5)).foregroundStyle(Konjo.fgDim)
                        Text("\(fmtHour12(maxx.quietHours.first ?? 23))–\(fmtHour12(maxx.quietHours.count > 1 ? maxx.quietHours[1] : 7))")
                            .font(Konjo.mono(10.5, weight: .semibold)).foregroundStyle(Konjo.fg)
                    }
                    // Unlike web (which leaves this bullet unconditional), gray it
                    // out when `headroomGate` is off — keeps this list visually
                    // consistent with `maxxSummary`, which already drops
                    // "headroom" from the summary line under the same condition.
                    Text("Nearing quota reset with high headroom")
                        .font(Konjo.mono(10.5))
                        .foregroundStyle(maxx.headroomGate ? Konjo.fgDim : Konjo.fgMute.opacity(0.5))
                }

                Text("current quota").font(Konjo.mono(8)).tracking(0.6).foregroundStyle(Konjo.fgDim)
                if let quotaError {
                    Text(quotaError).font(Konjo.mono(10)).foregroundStyle(Konjo.rose)
                } else {
                    quotaBar(label: "5h window", window: quota?.fiveHour, kind: .fiveHour, tint: Konjo.ice)
                    quotaBar(label: "7d window", window: quota?.sevenDay, kind: .sevenDay, tint: Konjo.jade)
                }
            }
        }
        .task { await loadQuota() }
    }

    // MARK: Quota bars

    private func quotaBar(label: String, window: QuotaWindow?, kind: LimitWindow, tint: Color) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            HStack {
                Text(label).font(Konjo.mono(9.5)).foregroundStyle(Konjo.fgDim)
                Spacer()
                Text(windowText(window, kind)).font(Konjo.mono(9.5)).foregroundStyle(Konjo.fg)
            }
            GeometryReader { geo in
                ZStack(alignment: .leading) {
                    RoundedRectangle(cornerRadius: 3).fill(Color.white.opacity(0.06))
                    RoundedRectangle(cornerRadius: 3).fill(tint)
                        .frame(width: geo.size.width * CGFloat(pct(window)) / 100)
                }
            }
            .frame(height: 5)
        }
    }

    // MARK: Toggle handler — create-on-first-enable → enable/disable

    private func toggle() {
        guard !busy else { return }
        busy = true
        error = nil
        let next = !maxx.enabled
        Task {
            do {
                var id = entryId
                if next {
                    if let existing = id {
                        try await model.client.enableMaxx(id: existing)
                    } else {
                        let created = try await model.client.createMaxx(MaxxBody(
                            name: goal.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? "maxx entry" : String(goal.prefix(60)),
                            goal: goal,
                            repo: repo,
                            enabled: true,
                            quietHours: maxx.quietHours,
                            headroomGate: maxx.headroomGate,
                            windows: maxx.windows.map(\.rawValue)))
                        id = created.id
                    }
                } else if let existing = id {
                    try await model.client.disableMaxx(id: existing)
                }
                onToggled(next, id)
            } catch {
                self.error = (error as? LopiError)?.errorDescription ?? "request failed"
            }
            busy = false
        }
    }

    private func loadQuota() async {
        do {
            quota = try await model.client.quota()
        } catch {
            quotaError = (error as? LopiError)?.errorDescription ?? "failed to load quota"
        }
    }

    // MARK: Formatting helpers (mirrors `MaxxPopover.svelte`'s `fmtHour12`/`pct`/`resetIn`/`resetOn`/`windowText`)

    private func fmtHour12(_ h: Int) -> String {
        let period = h < 12 ? "AM" : "PM"
        let h12 = h % 12 == 0 ? 12 : h % 12
        return "\(h12)\(period)"
    }

    private func pct(_ w: QuotaWindow?) -> Int {
        guard let w else { return 0 }
        return Int((w.utilization * 100).rounded())
    }

    /// "resets in 2h10m" from a unix-seconds `resetsAt`.
    private func resetIn(_ resetsAt: Int) -> String {
        let secs = max(0, resetsAt - Int(Date().timeIntervalSince1970))
        let h = secs / 3600
        let m = (secs % 3600) / 60
        return h > 0 ? "resets in \(h)h\(m)m" : "resets in \(m)m"
    }

    /// "resets on Thu 9AM" from a unix-seconds `resetsAt`.
    private func resetOn(_ resetsAt: Int) -> String {
        let d = Date(timeIntervalSince1970: TimeInterval(resetsAt))
        let cal = Calendar.current
        let weekdaySymbols = cal.shortWeekdaySymbols
        let weekday = weekdaySymbols[cal.component(.weekday, from: d) - 1]
        let hour = cal.component(.hour, from: d)
        return "resets on \(weekday) \(fmtHour12(hour))"
    }

    private func windowText(_ w: QuotaWindow?, _ kind: LimitWindow) -> String {
        guard let w else { return "no data yet" }
        guard let resetsAt = w.resetsAt else { return "\(pct(w))% · reset time unknown" }
        let resetText = kind == .fiveHour ? resetIn(resetsAt) : resetOn(resetsAt)
        return "\(pct(w))% · \(resetText)"
    }
}
