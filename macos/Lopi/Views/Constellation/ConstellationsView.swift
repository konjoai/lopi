import SwiftUI

/// Agent constellations: named groups with a routing strategy and weighted,
/// tagged members.
struct ConstellationsView: View {
    @Environment(AppModel.self) private var model
    @State private var rows: [ConstellationModel] = []
    @State private var loaded = false

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                if loaded && rows.isEmpty {
                    EmptyHint(
                        icon: "circle.hexagongrid",
                        text: "No constellations registered. They are re-created on each lopi sail start via POST /api/constellations."
                    )
                }
                ForEach(rows) { constellation in
                    card(constellation)
                }
            }
            .padding(20)
        }
        .background(Konjo.bg)
        .task {
            rows = await model.constellations()
            loaded = true
        }
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button {
                    Task { rows = await model.constellations() }
                } label: {
                    Label("Refresh", systemImage: "arrow.clockwise")
                }
            }
        }
    }

    private func card(_ c: ConstellationModel) -> some View {
        KonjoPanel {
            VStack(alignment: .leading, spacing: 10) {
                HStack {
                    Text(c.name)
                        .font(Konjo.sans(15, weight: .semibold))
                        .foregroundStyle(Konjo.fg)
                    Text(c.routingStrategy.kind)
                        .font(Konjo.mono(10))
                        .foregroundStyle(Konjo.konjo2)
                        .padding(.horizontal, 6)
                        .padding(.vertical, 2)
                        .background(Konjo.konjo.opacity(0.18))
                        .clipShape(Capsule())
                    Spacer()
                    Text("created \(DateFormatting.short(c.createdAt))")
                        .font(Konjo.mono(10))
                        .foregroundStyle(Konjo.fgMute)
                }
                ForEach(c.agents, id: \.agentId) { member in
                    HStack(spacing: 12) {
                        Circle().fill(Konjo.konjo).frame(width: 6, height: 6)
                        Text(member.agentId)
                            .font(Konjo.mono(11))
                            .foregroundStyle(Konjo.fg)
                        Text(String(format: "w %.1f", member.weight))
                            .font(Konjo.mono(10))
                            .foregroundStyle(Konjo.fgDim)
                        if member.maxConcurrent > 0 {
                            Text("max \(member.maxConcurrent)")
                                .font(Konjo.mono(10))
                                .foregroundStyle(Konjo.fgDim)
                        }
                        ForEach(member.tags, id: \.self) { tag in
                            Text(tag)
                                .font(Konjo.mono(9))
                                .foregroundStyle(Konjo.fgMute)
                                .padding(.horizontal, 5)
                                .padding(.vertical, 1)
                                .background(Konjo.bg2)
                                .clipShape(Capsule())
                        }
                        Spacer()
                    }
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}
