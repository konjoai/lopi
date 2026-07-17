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

/// Two arced loop-arrows facing opposite directions, in the shared 52×52
/// native space used by `LopiLogoMark` (matching the web `ICONS.mark`/
/// `SHELL_ICONS.mark` glyphs' path data point-for-point, quad-curves
/// standing in for the SVG's circular arcs same as `LopiMarkShape` does).
struct LopiLogoLoopShape: Shape {
    func path(in rect: CGRect) -> Path {
        let s = min(rect.width, rect.height) / 52
        func p(_ x: CGFloat, _ y: CGFloat) -> CGPoint {
            CGPoint(x: rect.minX + x * s, y: rect.minY + y * s)
        }
        var path = Path()

        path.move(to: p(12.5, 15.5))
        path.addLine(to: p(12.5, 14))
        path.addQuadCurve(to: p(18.5, 8), control: p(12.5, 8))
        path.addLine(to: p(39.5, 8))

        path.move(to: p(33.5, 2))
        path.addLine(to: p(39.5, 8))
        path.addLine(to: p(33.5, 14))

        path.move(to: p(39.5, 18.5))
        path.addLine(to: p(39.5, 20))
        path.addQuadCurve(to: p(33.5, 26), control: p(39.5, 26))
        path.addLine(to: p(12.5, 26))

        path.move(to: p(18.5, 32))
        path.addLine(to: p(12.5, 26))
        path.addLine(to: p(18.5, 20))

        return path
    }
}

/// The full lopi Stacks logo mark: `LopiLogoLoopShape` sitting above a
/// three-bar stack that fades toward the back, point-for-point matching the
/// web `ICONS.mark`/`SHELL_ICONS.mark` glyphs. Scoped to the unified
/// sidebar's Loop Stack row and each stack pane's header (not the app icon,
/// which keeps the plain boxed badge).
struct LopiLogoMark: View {
    var size: CGFloat = 24
    var color: Color = Konjo.flame

    var body: some View {
        let scale: CGFloat = size / 52
        return ZStack {
            LopiLogoLoopShape()
                .stroke(color, style: StrokeStyle(lineWidth: 2.85 * scale, lineCap: .round, lineJoin: .round))
                .frame(width: size, height: size)
            bar(y: 34, opacity: 0.9)
            bar(y: 40, opacity: 0.65)
            bar(y: 46, opacity: 0.4)
        }
        .frame(width: size, height: size)
    }

    /// One filled stack bar, fixed at `x: 8, width: 36, height: 4` in the
    /// shared 52×52 native space (matching the web SVG's `<rect>`s), then
    /// re-centered for SwiftUI's center-anchored `ZStack` offsets.
    private func bar(y: CGFloat, opacity: Double) -> some View {
        let scale: CGFloat = size / 52
        let width: CGFloat = 36 * scale
        let height: CGFloat = 4 * scale
        let cornerRadius: CGFloat = 2 * scale
        let offsetY: CGFloat = (y + 4 / 2 - 26) * scale
        return RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
            .fill(color.opacity(opacity))
            .frame(width: width, height: height)
            .offset(y: offsetY)
    }
}
