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

/// The bare lopi mark, stroked in flame orange with no backing tile — used
/// only as the "o" stand-in inside `LopiWordmark`, matching the web
/// wordmark's unboxed glyph.
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

/// The full lopi logo mark: `LopiMarkShape` boxed on a dark squircle with a
/// flame border, point-for-point matching `web/static/favicon.svg` and the
/// web `ICONS.mark`/`SHELL_ICONS.mark` glyphs. Stands in wherever a
/// "stack/block" icon or the app icon was doing duty as lopi's logo — the
/// unified sidebar's Forge row, per-pane headers, the app icon.
struct LopiLogoMark: View {
    var size: CGFloat = 24
    var color: Color = Konjo.flame
    var background: Color = Konjo.bg

    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: size * (5.25 / 24), style: .continuous)
                .fill(background)
                .frame(width: size * (21.0 / 24), height: size * (21.0 / 24))
            RoundedRectangle(cornerRadius: size * (4.5 / 24), style: .continuous)
                .stroke(color.opacity(0.85), lineWidth: size * (1.05 / 24))
                .frame(width: size * (18.9 / 24), height: size * (18.9 / 24))
            LopiMarkShape()
                .stroke(color, style: StrokeStyle(
                    lineWidth: size * (2.3 * 0.675 / 24), lineCap: .round, lineJoin: .round))
                .frame(width: size * 0.675, height: size * 0.675)
        }
        .frame(width: size, height: size)
    }
}
