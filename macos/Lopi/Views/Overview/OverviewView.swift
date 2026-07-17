import SwiftUI
import LopiStacksKit

/// Overview — the app-wide rollup. One dense, read-only row per live agent
/// across every pane and card, sortable by lifecycle with orb-colored status.
/// Clicking a row focuses that agent on the Forge grid. Mirrors web's
/// `/overview` route (`overviewRows`/`filterRows`/`filterCounts` in
/// `Store/Overview.swift` are the pure Swift port of `stores/overview.ts`).
///
/// Rows come only from the live `liveAgents` map — no fabricated rows.
struct OverviewView: View {
    @Environment(AppModel.self) private var model
    /// Focuses the given agent id on the Forge grid — supplied by `RootView`,
    /// which owns the `selection`/`PaneLayout` state this screen doesn't.
    var onFocus: (String) -> Void

    @State private var filter: OverviewFilter = .all

    private var rows: [OverviewRow] { overviewRows(Array(model.liveAgents.values)) }
    private var shown: [OverviewRow] { filterRows(rows, filter) }
    private var counts: [OverviewFilter: Int] { filterCounts(rows) }
    private var offline: Bool { model.connection == .offline || model.connection == .connecting }
    private var idle: Bool { model.connection == .live && rows.isEmpty }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                header
                filterChips
                content
            }
            .padding(24)
            .frame(maxWidth: 1200, alignment: .leading)
            .frame(maxWidth: .infinity)
        }
        .background(Konjo.bg)
    }

    private var header: some View {
        HStack(alignment: .firstTextBaseline) {
            Text("OVERVIEW")
                .font(Konjo.sans(24, weight: .bold))
                .foregroundStyle(Konjo.fg)
                .tracking(3)
            Spacer()
            ConnectionLED(state: model.connection)
        }
    }

    private var filterChips: some View {
        HStack(spacing: 8) {
            ForEach(OverviewFilter.allCases) { f in
                Button { filter = f } label: {
                    HStack(spacing: 5) {
                        Text(f.label).font(Konjo.mono(10.5, weight: filter == f ? .semibold : .regular))
                        Text("\(counts[f] ?? 0)").font(Konjo.mono(9)).foregroundStyle(Konjo.fgMute)
                    }
                    .foregroundStyle(filter == f ? Konjo.ice : Konjo.fgDim)
                    .padding(.horizontal, 11).padding(.vertical, 5)
                    .background(filter == f ? Konjo.ice.opacity(0.12) : Color.clear)
                    .overlay(Capsule().stroke(filter == f ? Konjo.ice.opacity(0.4) : Konjo.line, lineWidth: 1))
                    .clipShape(Capsule())
                }
                .buttonStyle(.plain)
            }
        }
    }

    @ViewBuilder private var content: some View {
        if offline {
            emptyState(model.connection == .connecting
                ? "backend offline — connecting to lopi sail…"
                : "start `lopi sail` to see live agents")
        } else if idle {
            emptyState("no live sessions — launch a run to populate the overview")
        } else if shown.isEmpty {
            emptyState("no \(filter.label) agents")
        } else {
            table
        }
    }

    private func emptyState(_ text: String) -> some View {
        Text(text)
            .font(Konjo.sans(13))
            .foregroundStyle(Konjo.fgMute)
            .padding(.vertical, 40)
            .frame(maxWidth: .infinity, alignment: .center)
    }

    // MARK: Table

    private var table: some View {
        VStack(alignment: .leading, spacing: 0) {
            tableHeader
            Rectangle().fill(Konjo.line).frame(height: 1)
            ForEach(shown) { row in
                rowView(row)
                Rectangle().fill(Konjo.line.opacity(0.5)).frame(height: 1)
            }
        }
        .konjoSurface(10, fill: Konjo.bg1.opacity(0.4))
    }

    private var tableHeader: some View {
        HStack(spacing: 12) {
            Color.clear.frame(width: 8)
            Text("goal").frame(maxWidth: .infinity, alignment: .leading)
            Text("repo · branch").frame(width: 180, alignment: .leading)
            Text("phase").frame(width: 140, alignment: .leading)
            Text("elapsed").frame(width: 70, alignment: .trailing)
            Text("cost").frame(width: 80, alignment: .trailing)
            Text("score").frame(width: 50, alignment: .trailing)
        }
        .font(Konjo.mono(9, weight: .semibold)).tracking(0.6).foregroundStyle(Konjo.fgMute)
        .padding(.horizontal, 12).padding(.vertical, 8)
    }

    private func rowView(_ row: OverviewRow) -> some View {
        HStack(spacing: 12) {
            statusDot(row)
            Text(row.goal).lineLimit(1).truncationMode(.tail)
                .foregroundStyle(Konjo.fg)
                .frame(maxWidth: .infinity, alignment: .leading)
            Text(repoBranchLabel(row)).lineLimit(1)
                .foregroundStyle(Konjo.fgDim)
                .frame(width: 180, alignment: .leading)
            phaseCell(row)
            Text(formatElapsed(row.elapsedMs)).foregroundStyle(Konjo.fgDim)
                .frame(width: 70, alignment: .trailing)
            Text(String(format: "$%.4f", row.cost)).foregroundStyle(Konjo.flame)
                .frame(width: 80, alignment: .trailing)
            scoreCell(row)
        }
        .font(Konjo.mono(11))
        .padding(.horizontal, 12).padding(.vertical, 9)
        .contentShape(Rectangle())
        .background(Color.white.opacity(0.0))
        .onTapGesture { onFocus(row.id) }
        .help("Open on the Forge grid")
        .accessibilityAddTraits(.isButton)
    }

    private func repoBranchLabel(_ row: OverviewRow) -> String {
        row.branch.isEmpty ? row.repo : "\(row.repo) · \(row.branch)"
    }

    private func statusDot(_ row: OverviewRow) -> some View {
        Circle()
            .fill(row.orbColor)
            .frame(width: 7, height: 7)
            .shadow(color: row.orbColor.opacity(row.awaiting ? 0.9 : 0.5), radius: row.awaiting ? 5 : 3)
            .frame(width: 8)
    }

    @ViewBuilder private func phaseCell(_ row: OverviewRow) -> some View {
        VStack(alignment: .leading, spacing: 1) {
            Text(row.phase).foregroundStyle(row.orbColor).lineLimit(1)
            if row.bucket != .running, row.bucket != .queued {
                Text(row.phase).font(Konjo.mono(8)).foregroundStyle(Konjo.fgMute)
            }
        }
        .frame(width: 140, alignment: .leading)
    }

    @ViewBuilder private func scoreCell(_ row: OverviewRow) -> some View {
        if let score = row.score {
            Text("\(Int((score * 100).rounded()))").foregroundStyle(overviewScoreColor(score))
                .frame(width: 50, alignment: .trailing)
        } else {
            Text("—").foregroundStyle(Konjo.fgMute).frame(width: 50, alignment: .trailing)
        }
    }
}
