import AppKit
import CoreGraphics
import ImageIO
import UniformTypeIdentifiers

// Renders the lopi "blue orb" app icon: a glossy ice-blue sphere with an
// upper-left light, fresnel rim, outer bloom, and a faint orbital ring, set on
// a dark Konjo squircle. Mirrors the Forge orb's ice palette (#00D4FF).
func render(_ S: CGFloat) -> CGImage {
    let cs = CGColorSpaceCreateDeviceRGB()
    let ctx = CGContext(
        data: nil, width: Int(S), height: Int(S), bitsPerComponent: 8, bytesPerRow: 0,
        space: cs, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    func col(_ r: CGFloat, _ g: CGFloat, _ b: CGFloat, _ a: CGFloat) -> CGColor {
        CGColor(red: r, green: g, blue: b, alpha: a)
    }

    // Dark squircle tile (Apple grid: ~80.5% content, ~22.4% corner radius).
    let margin = S * 0.094
    let rect = CGRect(x: margin, y: margin, width: S - 2 * margin, height: S - 2 * margin)
    let radius = rect.width * 0.2237
    let squircle = CGPath(
        roundedRect: rect, cornerWidth: radius, cornerHeight: radius, transform: nil)
    ctx.saveGState()
    ctx.addPath(squircle)
    ctx.clip()

    // Background: vertical near-black gradient + corner vignette.
    let bg = CGGradient(
        colorsSpace: cs, colors: [col(0.07, 0.07, 0.09, 1), col(0.02, 0.02, 0.03, 1)] as CFArray,
        locations: [0, 1])!
    ctx.drawLinearGradient(
        bg, start: CGPoint(x: rect.midX, y: rect.maxY),
        end: CGPoint(x: rect.midX, y: rect.minY), options: [])
    let vignette = CGGradient(
        colorsSpace: cs, colors: [col(0, 0, 0, 0), col(0, 0, 0, 0.45)] as CFArray,
        locations: [0.55, 1])!
    ctx.drawRadialGradient(
        vignette, startCenter: CGPoint(x: rect.midX, y: rect.midY), startRadius: 0,
        endCenter: CGPoint(x: rect.midX, y: rect.midY), endRadius: rect.width * 0.72, options: [])

    let cx = rect.midX
    let cy = rect.midY
    let r = rect.width * 0.30

    // Outer bloom (behind the sphere), additive.
    ctx.saveGState()
    ctx.setBlendMode(.plusLighter)
    let bloom = CGGradient(
        colorsSpace: cs,
        colors: [col(0.0, 0.85, 1.0, 0.30), col(0.0, 0.7, 1.0, 0.12), col(0, 0.5, 1, 0.0)]
            as CFArray, locations: [0.0, 0.45, 1.0])!
    ctx.drawRadialGradient(
        bloom, startCenter: CGPoint(x: cx, y: cy), startRadius: r * 0.6,
        endCenter: CGPoint(x: cx, y: cy), endRadius: r * 1.7, options: [])
    ctx.restoreGState()

    // Faint orbital ring behind the sphere (skip at tiny sizes — it just muddies).
    if S >= 128 {
        ctx.saveGState()
        ctx.setBlendMode(.plusLighter)
        ctx.setLineWidth(max(1, S * 0.0055))
        ctx.setStrokeColor(col(0.35, 0.88, 1.0, 0.32))
        let rw = r * 1.62
        let rh = r * 0.52
        ctx.addEllipse(in: CGRect(x: cx - rw, y: cy - rh, width: 2 * rw, height: 2 * rh))
        ctx.strokePath()
        ctx.restoreGState()
    }

    // Sphere body: offset radial gradient (light upper-left) for a 3-D read.
    ctx.saveGState()
    ctx.addEllipse(in: CGRect(x: cx - r, y: cy - r, width: 2 * r, height: 2 * r))
    ctx.clip()
    let lx = cx - r * 0.34
    let ly = cy + r * 0.40
    let body = CGGradient(
        colorsSpace: cs,
        colors: [
            col(0.92, 0.99, 1.0, 1.0),  // near-white core
            col(0.45, 0.90, 1.0, 1.0),  // light cyan
            col(0.0, 0.83, 1.0, 1.0),  // ice #00D4FF
            col(0.0, 0.42, 0.66, 1.0),  // mid blue
            col(0.02, 0.16, 0.30, 1.0),  // deep
            col(0.01, 0.07, 0.14, 1.0),  // edge
        ] as CFArray, locations: [0.0, 0.14, 0.34, 0.62, 0.85, 1.0])!
    ctx.drawRadialGradient(
        body, startCenter: CGPoint(x: lx, y: ly), startRadius: 0,
        endCenter: CGPoint(x: cx, y: cy), endRadius: r * 1.08, options: [.drawsAfterEndLocation])
    // Fresnel rim light, additive.
    ctx.setBlendMode(.plusLighter)
    let rim = CGGradient(
        colorsSpace: cs,
        colors: [
            col(0, 0.8, 1, 0.0), col(0, 0.8, 1, 0.0), col(0.5, 0.95, 1.0, 0.55),
            col(0.7, 0.98, 1.0, 0.0),
        ] as CFArray, locations: [0.0, 0.80, 0.945, 1.0])!
    ctx.drawRadialGradient(
        rim, startCenter: CGPoint(x: cx, y: cy), startRadius: 0,
        endCenter: CGPoint(x: cx, y: cy), endRadius: r, options: [])
    // Specular highlight.
    let sx = cx - r * 0.36
    let sy = cy + r * 0.42
    let spec = CGGradient(
        colorsSpace: cs, colors: [col(1, 1, 1, 0.9), col(1, 1, 1, 0.0)] as CFArray,
        locations: [0, 1])!
    ctx.drawRadialGradient(
        spec, startCenter: CGPoint(x: sx, y: sy), startRadius: 0,
        endCenter: CGPoint(x: sx, y: sy), endRadius: r * 0.42, options: [])
    ctx.restoreGState()

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
