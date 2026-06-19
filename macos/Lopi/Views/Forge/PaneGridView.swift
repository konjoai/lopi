import SwiftUI
#if canImport(AppKit)
import AppKit
#endif

/// Auto-tiling, drag-resizable pane grid. The shape is driven purely by
/// `count` (2 = halves, 3 = thirds, 4 = quarters, …) and every column/row
/// boundary carries a divider the operator can drag to bias the split.
struct PaneGridView<Pane: View>: View {
    let count: Int
    @ViewBuilder let pane: (Int) -> Pane

    @State private var colFr: [Double] = []
    @State private var rowFr: [Double] = []
    @State private var dragBase: [Double]?

    private let gap: CGFloat = 10
    private let pad: CGFloat = 10
    private let minFrac = 0.18

    var body: some View {
        GeometryReader { geo in
            let (cols, rows) = PaneLayout.dims(count)
            let cFr = even(colFr, cols)
            let rFr = even(rowFr, rows)
            let usableW = max(0, geo.size.width - pad * 2 - gap * CGFloat(cols - 1))
            let usableH = max(0, geo.size.height - pad * 2 - gap * CGFloat(rows - 1))
            let colW = sizes(cFr, usableW)
            let rowH = sizes(rFr, usableH)

            ZStack(alignment: .topLeading) {
                ForEach(0..<count, id: \.self) { idx in
                    let col = idx % cols
                    let row = idx / cols
                    pane(idx)
                        .frame(width: colW[col], height: rowH[row])
                        .offset(x: originX(col, colW), y: originY(row, rowH))
                        .transition(.scale(scale: 0.88).combined(with: .opacity))
                }
                ForEach(0..<max(0, cols - 1), id: \.self) { i in
                    divider(vertical: true)
                        .frame(width: 12, height: geo.size.height)
                        .position(x: originX(i, colW) + colW[i] + gap / 2, y: geo.size.height / 2)
                        .gesture(resize(fr: cFr, index: i, extent: usableW, isCol: true))
                }
                ForEach(0..<max(0, rows - 1), id: \.self) { i in
                    divider(vertical: false)
                        .frame(width: geo.size.width, height: 12)
                        .position(x: geo.size.width / 2, y: originY(i, rowH) + rowH[i] + gap / 2)
                        .gesture(resize(fr: rFr, index: i, extent: usableH, isCol: false))
                }
            }
            .padding(pad)
            // Parity with the web TileGrid: survivors spring to their new
            // tracks while the added/removed pane scales in/out. Keyed on
            // `count` so a gutter drag (which leaves count unchanged) never
            // fights the spring.
            .animation(.spring(response: 0.42, dampingFraction: 0.82), value: count)
            .onChange(of: count) { _, _ in colFr = []; rowFr = [] }
        }
    }

    // MARK: Geometry helpers

    private func even(_ fr: [Double], _ n: Int) -> [Double] {
        fr.count == n && n > 0 ? fr : Array(repeating: 1, count: max(n, 1))
    }

    private func sizes(_ fr: [Double], _ extent: CGFloat) -> [CGFloat] {
        let total = fr.reduce(0, +)
        guard total > 0 else { return fr.map { _ in 0 } }
        return fr.map { CGFloat($0 / total) * extent }
    }

    private func originX(_ col: Int, _ colW: [CGFloat]) -> CGFloat {
        pad + (0..<col).reduce(0) { $0 + colW[$1] + gap }
    }

    private func originY(_ row: Int, _ rowH: [CGFloat]) -> CGFloat {
        pad + (0..<row).reduce(0) { $0 + rowH[$1] + gap }
    }

    // MARK: Divider visuals

    private func divider(vertical: Bool) -> some View {
        ZStack {
            Color.clear.contentShape(Rectangle())
            RoundedRectangle(cornerRadius: 2)
                .fill(Konjo.konjo2.opacity(0.18))
                .frame(width: vertical ? 2 : nil, height: vertical ? nil : 2)
        }
        .onHover { inside in
            #if os(macOS)
            if inside { (vertical ? NSCursor.resizeLeftRight : NSCursor.resizeUpDown).push() }
            else { NSCursor.pop() }
            #endif
        }
    }

    // MARK: Resize gesture

    /// Build a drag gesture that re-biases two adjacent tracks.
    private func resize(fr: [Double], index: Int, extent: CGFloat, isCol: Bool) -> some Gesture {
        DragGesture(minimumDistance: 1)
            .onChanged { value in
                if dragBase == nil { dragBase = fr }
                guard let base = dragBase, extent > 0, base.count > index + 1 else { return }
                let total = base.reduce(0, +)
                let delta = Double((isCol ? value.translation.width : value.translation.height) / extent) * total
                let minV = minFrac * total
                var a = base[index] + delta
                var b = base[index + 1] - delta
                if a < minV { b -= minV - a; a = minV }
                if b < minV { a -= minV - b; b = minV }
                var next = base
                next[index] = a
                next[index + 1] = b
                if isCol { colFr = next } else { rowFr = next }
            }
            .onEnded { _ in dragBase = nil }
    }
}
