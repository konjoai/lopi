import SwiftUI

/// A smooth area sparkline with a gradient fill and a glowing leading point.
/// Normalizes against its own min/max so flat series still read cleanly.
struct Sparkline: View {
    let samples: [Double]
    var color: Color = Konjo.konjo2
    var showHead: Bool = true

    var body: some View {
        GeometryReader { geo in
            let pts = points(in: geo.size)
            ZStack {
                if pts.count >= 2 {
                    // Area fill.
                    areaPath(pts, height: geo.size.height)
                        .fill(LinearGradient(
                            colors: [color.opacity(0.35), color.opacity(0.02)],
                            startPoint: .top, endPoint: .bottom
                        ))
                    // Line.
                    linePath(pts)
                        .stroke(
                            LinearGradient(colors: [color.opacity(0.6), color],
                                           startPoint: .leading, endPoint: .trailing),
                            style: StrokeStyle(lineWidth: 2, lineCap: .round, lineJoin: .round)
                        )
                    // Leading head dot with glow.
                    if showHead, let last = pts.last {
                        Circle()
                            .fill(color)
                            .frame(width: 6, height: 6)
                            .position(last)
                            .konjoGlow(color, radius: 6)
                    }
                } else {
                    Rectangle().fill(Konjo.line)
                        .frame(height: 1)
                        .position(x: geo.size.width / 2, y: geo.size.height / 2)
                }
            }
            .animation(.easeOut(duration: 0.4), value: samples)
        }
    }

    private func points(in size: CGSize) -> [CGPoint] {
        guard samples.count >= 2 else { return [] }
        let lo = samples.min() ?? 0
        let hi = samples.max() ?? 1
        let span = max(hi - lo, 0.0001)
        let dx = size.width / CGFloat(samples.count - 1)
        return samples.enumerated().map { i, v in
            let y = size.height * (1 - CGFloat((v - lo) / span))
            return CGPoint(x: CGFloat(i) * dx, y: y)
        }
    }

    private func linePath(_ pts: [CGPoint]) -> Path {
        var p = Path()
        p.move(to: pts[0])
        for pt in pts.dropFirst() { p.addLine(to: pt) }
        return p
    }

    private func areaPath(_ pts: [CGPoint], height: CGFloat) -> Path {
        var p = linePath(pts)
        if let last = pts.last, let first = pts.first {
            p.addLine(to: CGPoint(x: last.x, y: height))
            p.addLine(to: CGPoint(x: first.x, y: height))
            p.closeSubpath()
        }
        return p
    }
}

/// A thin circular gauge (0...1) with a centered caption — used for context
/// pressure on cognition cards.
struct PressureRing: View {
    var value: Double
    var label: String
    var size: CGFloat = 54
    var color: Color = Konjo.konjo2

    var body: some View {
        ZStack {
            Circle().stroke(Konjo.line2, lineWidth: 5)
            Circle()
                .trim(from: 0, to: min(max(value, 0), 1))
                .stroke(color, style: StrokeStyle(lineWidth: 5, lineCap: .round))
                .rotationEffect(.degrees(-90))
                .animation(.easeOut(duration: 0.5), value: value)
            VStack(spacing: 0) {
                Text("\(Int(value * 100))")
                    .font(Konjo.mono(13, weight: .semibold))
                    .foregroundStyle(Konjo.fg)
                Text(label)
                    .font(Konjo.mono(7))
                    .foregroundStyle(Konjo.fgMute)
            }
        }
        .frame(width: size, height: size)
    }
}

/// A horizontal labeled meter (0...1) with a colored fill.
struct Meter: View {
    var value: Double
    var color: Color
    var height: CGFloat = 5

    var body: some View {
        GeometryReader { geo in
            ZStack(alignment: .leading) {
                Capsule().fill(Konjo.line2)
                Capsule()
                    .fill(color)
                    .frame(width: geo.size.width * CGFloat(min(max(value, 0), 1)))
                    .animation(.easeOut(duration: 0.4), value: value)
            }
        }
        .frame(height: height)
    }
}
