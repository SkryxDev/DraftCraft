// Draws the DMG window's background: packaging/macos/dmg/background.png (660 × 420 pt) and
// background@2x.png, which package.sh joins into the volume's background.tiff. Our own art: the DraftCraft
// logo (docs/brand/draftcraft-logo.svg), a light blueprint grid, two cards behind the app and the
// Applications link that Finder draws on top, and a compass arc from one to the other.
//
//   swift packaging/macos/dmg/background.swift <font dir> <out dir>
//
// <font dir> holds Inter-Medium.ttf and Inter-Regular.ttf (Inter, SIL OFL 1.1,
// https://github.com/rsms/inter): only the rendered text ends up in the image. Keep the geometry in
// step with make-layout.sh (window size, icon centres).

import AppKit
import CoreText

let args = CommandLine.arguments
guard args.count == 3 else {
    FileHandle.standardError.write("usage: swift background.swift <font dir> <out dir>\n".data(using: .utf8)!)
    exit(2)
}
let root = URL(fileURLWithPath: #filePath).deletingLastPathComponent().appendingPathComponent("../../..").standardized
let fontDir = URL(fileURLWithPath: args[1])
let outDir = URL(fileURLWithPath: args[2])

for name in ["Inter-Medium.ttf", "Inter-Regular.ttf"] {
    var error: Unmanaged<CFError>?
    if !CTFontManagerRegisterFontsForURL(fontDir.appendingPathComponent(name) as CFURL, .process, &error) {
        FileHandle.standardError.write("can't load \(name)\n".data(using: .utf8)!)
        exit(1)
    }
}

// Geometry in points; make-layout.sh puts the icons at these centres.
let size = NSSize(width: 660, height: 420)
let appCentre = NSPoint(x: 170, y: 236)
let appsCentre = NSPoint(x: 490, y: 236)
let card = NSSize(width: 184, height: 184)
let cardCentreY: CGFloat = 250

// The brand's blueprint palette.
func rgb(_ hex: UInt32, _ alpha: CGFloat = 1) -> NSColor {
    NSColor(srgbRed: CGFloat((hex >> 16) & 0xff) / 255, green: CGFloat((hex >> 8) & 0xff) / 255, blue: CGFloat(hex & 0xff) / 255, alpha: alpha)
}
let navy = rgb(0x0e3463)
let accent = rgb(0x3ea6e8)

func draw(scale: CGFloat) -> NSBitmapImageRep {
    let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: Int(size.width * scale), pixelsHigh: Int(size.height * scale),
                               bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
                               colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
    rep.size = size
    NSGraphicsContext.saveGraphicsState()
    let context = NSGraphicsContext(bitmapImageRep: rep)!
    NSGraphicsContext.current = context
    // Top-left origin, like Finder's icon positions.
    let flip = NSAffineTransform()
    flip.translateX(by: 0, yBy: size.height)
    flip.scaleX(by: 1, yBy: -1)
    flip.concat()

    // Paper: a soft blue-white wash with a glow behind the title.
    NSGradient(colors: [rgb(0xf7fbff), rgb(0xe3eefa)])!.draw(in: NSRect(origin: .zero, size: size), angle: 90)
    NSGradient(colors: [rgb(0xffffff, 0.85), rgb(0xffffff, 0)])!
        .draw(fromCenter: NSPoint(x: 330, y: 70), radius: 0, toCenter: NSPoint(x: 330, y: 70), radius: 300, options: [])

    // Drafting grid: minor lines every 20 pt, major every 100 pt.
    for (step, alpha) in [(CGFloat(20), CGFloat(0.07)), (100, 0.10)] {
        let path = NSBezierPath()
        var x: CGFloat = 30
        while x < size.width { path.move(to: NSPoint(x: x, y: 0)); path.line(to: NSPoint(x: x, y: size.height)); x += step }
        var y: CGFloat = 10
        while y < size.height { path.move(to: NSPoint(x: 0, y: y)); path.line(to: NSPoint(x: size.width, y: y)); y += step }
        path.lineWidth = 1 / scale
        accent.withAlphaComponent(alpha).setStroke()
        path.stroke()
    }

    // Images and text draw upright: undo the flip about the horizontal line at `y`.
    func upright(at y: CGFloat, _ body: () -> Void) {
        NSGraphicsContext.saveGraphicsState()
        let unflip = NSAffineTransform()
        unflip.translateX(by: 0, yBy: y * 2)
        unflip.scaleX(by: 1, yBy: -1)
        unflip.concat()
        body()
        NSGraphicsContext.restoreGraphicsState()
    }

    // Logo, centred at the top.
    let logo = NSImage(contentsOf: root.appendingPathComponent("docs/brand/draftcraft-logo.svg"))!
    let logoHeight: CGFloat = 50
    let logoWidth = logoHeight * logo.size.width / logo.size.height
    let logoRect = NSRect(x: (size.width - logoWidth) / 2, y: 34, width: logoWidth, height: logoHeight)
    upright(at: logoRect.midY) { logo.draw(in: logoRect) }

    func text(_ string: String, font: String, size pt: CGFloat, color: NSColor, centreY: CGFloat) {
        let attributes: [NSAttributedString.Key: Any] = [.font: NSFont(name: font, size: pt)!, .foregroundColor: color]
        let s = NSAttributedString(string: string, attributes: attributes)
        let bounds = s.size()
        upright(at: centreY) { s.draw(at: NSPoint(x: (size.width - bounds.width) / 2, y: centreY - bounds.height / 2)) }
    }
    text("Drag DraftCraft into Applications to install", font: "Inter-Medium", size: 15, color: rgb(0x3d5a7d), centreY: 116)

    // Cards behind the two icons (Finder draws the icons and their labels on top).
    for centre in [appCentre, appsCentre] {
        let rect = NSRect(x: centre.x - card.width / 2, y: cardCentreY - card.height / 2, width: card.width, height: card.height)
        let path = NSBezierPath(roundedRect: rect, xRadius: 40, yRadius: 40)
        NSGraphicsContext.saveGraphicsState()
        let shadow = NSShadow()
        shadow.shadowColor = navy.withAlphaComponent(0.12)
        shadow.shadowBlurRadius = 22
        shadow.shadowOffset = NSSize(width: 0, height: -8)
        shadow.set()
        rgb(0xffffff, 0.72).setFill()
        path.fill()
        NSGraphicsContext.restoreGraphicsState()
        path.lineWidth = 1.5
        rgb(0xffffff, 0.95).setStroke()
        path.stroke()
    }

    // A compass arc from the app to Applications, drawn in dots, ending in an arrowhead.
    let from = NSPoint(x: appCentre.x + card.width / 2 + 18, y: 252)
    let to = NSPoint(x: appsCentre.x - card.width / 2 - 18, y: 252)
    let control1 = NSPoint(x: from.x + 16, y: from.y - 46)
    let control2 = NSPoint(x: to.x - 16, y: to.y - 46)
    let arc = NSBezierPath()
    arc.move(to: from)
    arc.curve(to: to, controlPoint1: control1, controlPoint2: control2)
    arc.lineWidth = 3.2
    arc.lineCapStyle = .round
    arc.setLineDash([0.01, 9], count: 2, phase: 0)
    accent.setStroke()
    arc.stroke()
    // Arrowhead along the curve's end tangent (from the last control point to the end).
    let (dx, dy) = (to.x - control2.x, to.y - control2.y)
    let length = (dx * dx + dy * dy).squareRoot()
    let (ux, uy) = (dx / length, dy / length)
    let (back, spread): (CGFloat, CGFloat) = (12, 8)
    let head = NSBezierPath()
    head.move(to: NSPoint(x: to.x - ux * back - uy * spread, y: to.y - uy * back + ux * spread))
    head.line(to: to)
    head.line(to: NSPoint(x: to.x - ux * back + uy * spread, y: to.y - uy * back - ux * spread))
    head.lineWidth = 3.2
    head.lineCapStyle = .round
    head.lineJoinStyle = .round
    head.stroke()
    // The compass point the arc starts from.
    let pivot = NSBezierPath(ovalIn: NSRect(x: from.x - 4, y: from.y - 4, width: 8, height: 8))
    navy.withAlphaComponent(0.85).setFill()
    pivot.fill()

    text("Free software · MIT or Apache 2.0 · github.com/SkryxDev/DraftCraft", font: "Inter-Regular", size: 12, color: rgb(0x7389a3), centreY: 374)

    NSGraphicsContext.restoreGraphicsState()
    return rep
}

try FileManager.default.createDirectory(at: outDir, withIntermediateDirectories: true)
for (scale, name) in [(CGFloat(1), "background.png"), (2, "background@2x.png")] {
    let data = draw(scale: scale).representation(using: .png, properties: [:])!
    try data.write(to: outDir.appendingPathComponent(name))
}
print("wrote \(outDir.path)/background.png and background@2x.png")
