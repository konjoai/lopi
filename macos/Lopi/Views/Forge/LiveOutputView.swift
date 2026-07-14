import SwiftUI

/// LiveOutputView — the sectioned live-output attachment fused under a running
/// card, mirroring the web `StackOutput`. Collapsed it's a single live line
/// (dot + latest kind + latest text + expand); expanded it's a filter row plus
/// four independently-collapsible sections — thinking / actions / tools /
/// output — categorised from the same `TranscriptBlock` feed the flat
/// transcript used. Renders nothing when there are no blocks yet.
struct LiveOutputView: View {
    var blocks: [TranscriptBlock]
    var streaming: Bool

    /// The four output buckets, in render order.
    enum Kind: String, CaseIterable, Hashable { case thinking, actions, tools, output }
    /// The filter chips — `all` plus one per kind.
    enum Filter: String, CaseIterable, Hashable { case all, thinking, actions, tools, output }

    @State private var expanded = false
    @State private var filter: Filter = .all
    @State private var open: [Kind: Bool] = [.thinking: true, .actions: false, .tools: false, .output: false]

    var body: some View {
        if !blocks.isEmpty {
            VStack(spacing: 0) {
                if expanded { expandedBody } else { collapsedStrip }
            }
            .background(Konjo.deep)
            .overlay(Rectangle().stroke(Konjo.flame.opacity(0.45), lineWidth: 1).mask(edgesMask))
            .clipShape(UnevenRoundedRectangle(bottomLeadingRadius: 9, bottomTrailingRadius: 9))
            .padding(.top, 6)
        }
    }

    /// Border on every edge except the top (the card owns the shared seam).
    private var edgesMask: some View {
        VStack(spacing: 0) { Color.clear.frame(height: 1); Color.black }
    }

    // MARK: Collapsed — one live line

    private var collapsedStrip: some View {
        let latest = blocks.last
        return HStack(spacing: 8) {
            liveDot
            if let latest {
                Text(kind(of: latest).rawValue).font(Konjo.mono(10)).foregroundStyle(Konjo.violet)
            }
            Text(latest.map(text(of:)) ?? "").font(Konjo.mono(10)).foregroundStyle(Konjo.fgDim)
                .lineLimit(1).truncationMode(.tail)
            Spacer(minLength: 0)
            miniButton("arrow.down.left.and.arrow.up.right", accent: Konjo.ice) { expanded = true }
        }
        .padding(.horizontal, 12).padding(.vertical, 8)
    }

    // MARK: Expanded — filter row + sections

    private var expandedBody: some View {
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                HStack(spacing: 6) { liveDot; Text("LIVE OUTPUT").font(Konjo.mono(9)).tracking(0.8).foregroundStyle(Konjo.ice) }
                Spacer(minLength: 0)
                ForEach(Filter.allCases, id: \.self) { f in filterChip(f) }
                miniButton("arrow.up.right.and.arrow.down.left", accent: Konjo.flame) { expanded = false }
            }
            .padding(.horizontal, 12).padding(.vertical, 7)
            .overlay(Rectangle().fill(Konjo.ice.opacity(0.1)).frame(height: 1), alignment: .bottom)

            ScrollView {
                VStack(spacing: 0) {
                    ForEach(Kind.allCases, id: \.self) { k in
                        if filter == .all || filter.rawValue == k.rawValue { section(k) }
                    }
                }
            }
            .frame(maxHeight: 340)
        }
    }

    private func section(_ k: Kind) -> some View {
        let items = byKind[k] ?? []
        let isOpen = filter.rawValue == k.rawValue || (open[k] ?? false)
        return VStack(alignment: .leading, spacing: 0) {
            Button { open[k, default: false].toggle() } label: {
                HStack(spacing: 8) {
                    Image(systemName: "chevron.down").font(.system(size: 9, weight: .bold))
                        .rotationEffect(.degrees(isOpen ? 180 : 0)).foregroundStyle(Konjo.fgMute)
                    Image(systemName: icon(k)).font(.system(size: 11))
                    Text(k.rawValue).font(Konjo.mono(10.5))
                    Spacer(minLength: 0)
                    Text("\(items.count)").font(Konjo.mono(9)).foregroundStyle(Konjo.fgMute)
                }
                .foregroundStyle(accent(k))
                .padding(.horizontal, 12).padding(.vertical, 9)
            }
            .buttonStyle(.plain)
            if isOpen {
                VStack(alignment: .leading, spacing: 4) {
                    ForEach(items) { b in
                        Text(text(of: b)).font(Konjo.mono(10.5))
                            .italic(k == .thinking)
                            .foregroundStyle(lineColor(k))
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
                .padding(.leading, 32).padding(.trailing, 12).padding(.bottom, 11).padding(.top, 2)
            }
        }
        .overlay(Rectangle().fill(Konjo.ice.opacity(0.06)).frame(height: 1), alignment: .top)
    }

    // MARK: Chrome bits

    private var liveDot: some View {
        Circle().fill(Konjo.ice).frame(width: 6, height: 6).shadow(color: Konjo.ice, radius: 3)
    }

    private func filterChip(_ f: Filter) -> some View {
        let on = filter == f
        return Button { filter = f } label: {
            Text(f.rawValue.uppercased()).font(Konjo.mono(9))
                .foregroundStyle(on ? Konjo.ice : Konjo.fgMute)
                .padding(.horizontal, 7).padding(.vertical, 2)
                .background(RoundedRectangle(cornerRadius: 3).fill(Konjo.ice.opacity(on ? 0.06 : 0)))
                .overlay(RoundedRectangle(cornerRadius: 3).stroke(Konjo.ice.opacity(on ? 0.3 : 0), lineWidth: 1))
        }
        .buttonStyle(.plain)
    }

    private func miniButton(_ systemImage: String, accent: Color, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: systemImage).font(.system(size: 11))
                .foregroundStyle(accent).frame(width: 24, height: 22)
                .overlay(RoundedRectangle(cornerRadius: 5).stroke(accent.opacity(0.4), lineWidth: 1))
        }
        .buttonStyle(.plain)
    }

    // MARK: Categorisation (mirrors web `StackOutput.categorize`)

    private var byKind: [Kind: [TranscriptBlock]] { Dictionary(grouping: blocks, by: { kind(of: $0) }) }

    private func kind(of b: TranscriptBlock) -> Kind {
        switch b {
        case .thinking: return .thinking
        case .toolCall: return .tools
        case .status: return .actions
        case .assistantText: return .output
        }
    }

    private func text(of b: TranscriptBlock) -> String {
        switch b {
        case let .assistantText(_, t, _): return t
        case let .thinking(_, t): return t
        case let .status(_, _, label): return label
        case let .toolCall(_, tool, args): return args.isEmpty ? tool : "\(tool) → \(args)"
        }
    }

    private func icon(_ k: Kind) -> String {
        switch k {
        case .thinking: return "lightbulb"
        case .actions: return "bolt"
        case .tools: return "wrench.and.screwdriver"
        case .output: return "list.bullet"
        }
    }

    private func accent(_ k: Kind) -> Color {
        switch k {
        case .thinking: return Konjo.violet
        case .actions: return Konjo.sun
        case .tools: return Konjo.ice
        case .output: return Konjo.jade
        }
    }

    private func lineColor(_ k: Kind) -> Color {
        switch k {
        case .thinking: return Konjo.violet.opacity(0.72)
        case .output: return Konjo.jade.opacity(0.75)
        case .actions, .tools: return Konjo.fgDim
        }
    }
}
