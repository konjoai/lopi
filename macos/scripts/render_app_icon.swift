import AppKit
import CoreGraphics
import ImageIO
import UniformTypeIdentifiers

// Renders the lopi "loop stack" app icon: a dark squircle tile bordered in
// flame orange, holding the loop-stack mark (two rounded arrows forming a
// cycle) in the same orange. Mirrors `web/static/favicon.svg` and
// `Components/LopiMark.swift` point-for-point so the icon, the in-app pane
// logos, and the wordmark's "o" all read as one glyph.
func render(_ S: CGFloat) -> CGImage {
    let cs = CGColorSpaceCreateDeviceRGB()
    let ctx = CGContext(
        data: nil, width: Int(S), height: Int(S), bitsPerComponent: 8, bytesPerRow: 0,
        space: cs, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    func col(_ r: CGFloat, _ g: CGFloat, _ b: CGFloat, _ a: CGFloat) -> CGColor {
        CGColor(red: r, green: g, blue: b, alpha: a)
    }

    let flame = col(1.0, 0.584, 0.0, 1.0) // #FF9500 — Konjo.flame
    let black = col(0.039, 0.039, 0.039, 1.0) // #0A0A0A — Konjo.bg

    // Dark squircle tile — proportions match `favicon.svg`'s 32pt canvas
    // (content 87.5% of the tile, corner radius 25% of content).
    let margin = S * (2.0 / 32.0)
    let rect = CGRect(x: margin, y: margin, width: S - 2 * margin, height: S - 2 * margin)
    let radius = rect.width * 0.25
    let squircle = CGPath(roundedRect: rect, cornerWidth: radius, cornerHeight: radius, transform: nil)
    ctx.saveGState()
    ctx.addPath(squircle)
    ctx.clip()
    ctx.setFillColor(black)
    ctx.fill(rect)
    ctx.restoreGState()

    // Border ring, inset from the tile edge.
    let borderInset = S * (1.4 / 32.0)
    let borderWidth = S * (1.4 / 32.0)
    let borderRect = rect.insetBy(dx: borderInset, dy: borderInset)
    let borderRadius = radius * (borderRect.width / rect.width)
    let borderPath = CGPath(
        roundedRect: borderRect, cornerWidth: borderRadius, cornerHeight: borderRadius, transform: nil)
    ctx.saveGState()
    ctx.setStrokeColor(flame)
    ctx.setAlpha(0.85)
    ctx.setLineWidth(borderWidth)
    ctx.addPath(borderPath)
    ctx.strokePath()
    ctx.restoreGState()

    // Loop-stack mark: two rounded arrows forming a cycle, drawn in a local
    // 24×24 coordinate space scaled/centered to match `LopiMarkShape`.
    let unit = S * (0.9 / 32.0)
    let originX = S * (5.2 / 32.0)
    let originY = S * (5.2 / 32.0)
    func m(_ x: CGFloat, _ y: CGFloat) -> CGPoint {
        CGPoint(x: originX + x * unit, y: S - (originY + y * unit))
    }

    ctx.saveGState()
    ctx.setStrokeColor(flame)
    ctx.setLineWidth(S * (2.3 / 32.0))
    ctx.setLineCap(.round)
    ctx.setLineJoin(.round)

    let top = CGMutablePath()
    top.move(to: m(17, 2))
    top.addLine(to: m(21, 6))
    top.addLine(to: m(17, 10))
    ctx.addPath(top)
    ctx.strokePath()

    let topConnector = CGMutablePath()
    topConnector.move(to: m(3, 11))
    topConnector.addLine(to: m(3, 10))
    topConnector.addQuadCurve(to: m(7, 6), control: m(3, 6))
    topConnector.addLine(to: m(21, 6))
    ctx.addPath(topConnector)
    ctx.strokePath()

    let bottom = CGMutablePath()
    bottom.move(to: m(7, 22))
    bottom.addLine(to: m(3, 18))
    bottom.addLine(to: m(7, 14))
    ctx.addPath(bottom)
    ctx.strokePath()

    let bottomConnector = CGMutablePath()
    bottomConnector.move(to: m(21, 13))
    bottomConnector.addLine(to: m(21, 14))
    bottomConnector.addQuadCurve(to: m(17, 18), control: m(21, 18))
    bottomConnector.addLine(to: m(3, 18))
    ctx.addPath(bottomConnector)
    ctx.strokePath()
    ctx.restoreGState()

    return ctx.makeImage()!
}

func writePNG(_ image: CGImage, _ path: String) {
    let url = URL(fileURLWithPath: path)
    let dest = CGImageDestinationCreateWithURL(url as CFURL, UTType.png.identifier as CFString, 1, nil)!
    CGImageDestinationAddImage(dest, image, nil)
    CGImageDestinationFinalize(dest)
}

let outDir = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "/tmp/lopi_icons"
try? FileManager.default.createDirectory(atPath: outDir, withIntermediateDirectories: true)
for s in [16, 32, 64, 128, 256, 512, 1024] {
    writePNG(render(CGFloat(s)), "\(outDir)/icon_\(s).png")
    print("wrote icon_\(s).png")
}
