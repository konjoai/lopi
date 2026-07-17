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

/// The full lopi Stacks logo mark: the boxed `LopiMarkShape` loop badge
/// overlapping the top-right corner of a three-bar stack that fades toward
/// the back, point-for-point matching the web `ICONS.mark`/`SHELL_ICONS.mark`
/// glyphs. Scoped to the unified sidebar's Loop Stack row and each stack
/// pane's header (not the app icon, which keeps the plain boxed badge).
struct LopiLogoMark: View {
    var size: CGFloat = 24
    var color: Color = Konjo.flame
    var background: Color = Konjo.bg

    var body: some View {
        let scale: CGFloat = size / 24
        let badgeSize: CGFloat = size * 0.4
        let badgeOffsetX: CGFloat = 6.9 * scale
        let badgeOffsetY: CGFloat = -6.4 * scale
        return ZStack {
            bar(y: 19.1, opacity: 0.3)
            bar(y: 13.7, opacity: 0.58)
            bar(y: 8.3, opacity: 1)
            badge(boxSize: badgeSize)
                .offset(x: badgeOffsetX, y: badgeOffsetY)
        }
        .frame(width: size, height: size)
    }

    /// One stroked stack bar, fixed at `x: 0.9, width: 15.9, height: 4` in the
    /// shared 24×24 native space (matching the web SVG's `<rect>`s), then
    /// re-centered for SwiftUI's center-anchored `ZStack` offsets.
    private func bar(y: CGFloat, opacity: Double) -> some View {
        let scale: CGFloat = size / 24
        let width: CGFloat = 15.9 * scale
        let height: CGFloat = 4.0 * scale
        let cornerRadius: CGFloat = 2.0 * scale
        let strokeWidth: CGFloat = 1.15 * scale
        let offsetX: CGFloat = (0.9 + 15.9 / 2 - 12) * scale
        let offsetY: CGFloat = (y + 4.0 / 2 - 12) * scale
        return RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
            .stroke(color.opacity(opacity), lineWidth: strokeWidth)
            .frame(width: width, height: height)
            .offset(x: offsetX, y: offsetY)
    }

    /// The boxed loop badge at a given rendered size — the pre-Stacks-rebrand
    /// `LopiLogoMark` body, now reused as the corner badge.
    private func badge(boxSize: CGFloat) -> some View {
        ZStack {
            RoundedRectangle(cornerRadius: boxSize * (5.25 / 24), style: .continuous)
                .fill(background)
                .frame(width: boxSize * (21.0 / 24), height: boxSize * (21.0 / 24))
            RoundedRectangle(cornerRadius: boxSize * (4.5 / 24), style: .continuous)
                .stroke(color.opacity(0.85), lineWidth: boxSize * (1.05 / 24))
                .frame(width: boxSize * (18.9 / 24), height: boxSize * (18.9 / 24))
            LopiMarkShape()
                .stroke(color, style: StrokeStyle(
                    lineWidth: boxSize * (2.3 * 0.675 / 24), lineCap: .round, lineJoin: .round))
                .frame(width: boxSize * 0.675, height: boxSize * 0.675)
        }
        .frame(width: boxSize, height: boxSize)
    }
}
