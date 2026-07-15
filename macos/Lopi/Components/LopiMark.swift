import SwiftUI

/// The lopi loop-stack mark — two rounded arrows forming a cycle, matching
/// the web icon set's `loop` glyph (`web/src/lib/components/stacks/icons.ts`)
/// point-for-point in its 24×24 source space, scaled to fit `rect`.
struct LopiMarkShape: Shape {
    func path(in rect: CGRect) -> Path {
        let s = min(rect.width, rect.height) / 24
        func p(_ x: CGFloat, _ y: CGFloat) -> CGPoint {
            CGPoint(x: rect.minX + x * s, y: rect.minY + y * s)
        }
        var path = Path()
        path.move(to: p(17, 2))
        path.addLine(to: p(21, 6))
        path.addLine(to: p(17, 10))

        path.move(to: p(3, 11))
        path.addLine(to: p(3, 10))
        path.addQuadCurve(to: p(7, 6), control: p(3, 6))
        path.addLine(to: p(21, 6))

        path.move(to: p(7, 22))
        path.addLine(to: p(3, 18))
        path.addLine(to: p(7, 14))

        path.move(to: p(21, 13))
        path.addLine(to: p(21, 14))
        path.addQuadCurve(to: p(17, 18), control: p(21, 18))
        path.addLine(to: p(3, 18))
        return path
    }
}

/// The lopi mark, stroked in flame orange — stands in for the old
/// "square.grid.2x2" block glyph wherever it was doing duty as the product's
/// logo (pane headers, the app icon).
struct LopiMark: View {
    var size: CGFloat = 20
    var color: Color = Konjo.flame

    var body: some View {
        LopiMarkShape()
            .stroke(color, style: StrokeStyle(lineWidth: max(1.4, size * 0.09), lineCap: .round, lineJoin: .round))
            .frame(width: size, height: size)
    }
}

/// The full "lopi" wordmark: plain letters either side of `LopiMark` standing
/// in for the "o", mirroring the lockup used on web (`LopiWordmark.svelte`).
struct LopiWordmark: View {
    var fontSize: CGFloat = 15
    var weight: Font.Weight = .bold
    var color: Color = Konjo.fg
    var markColor: Color = Konjo.flame

    var body: some View {
        HStack(spacing: fontSize * 0.03) {
            Text("l").font(Konjo.sans(fontSize, weight: weight)).foregroundStyle(color)
            LopiMark(size: fontSize * 1.05, color: markColor)
            Text("pi").font(Konjo.sans(fontSize, weight: weight)).foregroundStyle(color)
        }
    }
}
