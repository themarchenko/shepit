// Renders the background of the release disk image into dmg/background.png and dmg/background@2x.png.
//
//   swift scripts/make-dmg-background.swift
//
// Finder draws the two icons and their names on top; their positions are in dmg/settings.py, and the
// arrow and text here are laid out around them. Change both together.
import AppKit
import Foundation

let size = CGSize(width: 660, height: 420)
let appCenter = CGPoint(x: 170, y: 170)
let applicationsCenter = CGPoint(x: 490, y: 170)

let offWhite = NSColor(red: 0.957, green: 0.949, blue: 0.933, alpha: 1)

func render(scale: CGFloat) -> NSBitmapImageRep {
    let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: Int(size.width * scale),
                               pixelsHigh: Int(size.height * scale), bitsPerSample: 8, samplesPerPixel: 4,
                               hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
                               bytesPerRow: 0, bitsPerPixel: 0)!
    rep.size = size
    NSGraphicsContext.saveGraphicsState()
    let context = NSGraphicsContext(bitmapImageRep: rep)!
    NSGraphicsContext.current = context
    // Top-left origin, like Finder's icon positions.
    let cg = context.cgContext
    cg.translateBy(x: 0, y: size.height)
    cg.scaleBy(x: 1, y: -1)
    let flipped = NSGraphicsContext(cgContext: cg, flipped: true)
    NSGraphicsContext.current = flipped

    // Black glass: near black with a soft light from the top.
    NSColor(red: 0.07, green: 0.07, blue: 0.078, alpha: 1).setFill()
    CGRect(origin: .zero, size: size).fill()
    let glow = NSGradient(colors: [NSColor.white.withAlphaComponent(0.07), NSColor.white.withAlphaComponent(0)])!
    glow.draw(fromCenter: CGPoint(x: size.width / 2, y: -40), radius: 0,
              toCenter: CGPoint(x: size.width / 2, y: -40), radius: 420, options: [])

    text("Drag Shepit into Applications", at: CGPoint(x: size.width / 2, y: 58),
         font: .systemFont(ofSize: 17, weight: .semibold), color: offWhite, centered: true)

    // Arrow between the icons.
    let y = appCenter.y
    let from = appCenter.x + 82, to = applicationsCenter.x - 82
    let line = NSBezierPath()
    line.move(to: CGPoint(x: from, y: y))
    line.line(to: CGPoint(x: to, y: y))
    line.move(to: CGPoint(x: to - 11, y: y - 9))
    line.line(to: CGPoint(x: to, y: y))
    line.line(to: CGPoint(x: to - 11, y: y + 9))
    line.lineWidth = 2.5
    line.lineCapStyle = .round
    line.lineJoinStyle = .round
    offWhite.withAlphaComponent(0.45).setStroke()
    line.stroke()

    // What to do when macOS stops the first launch.
    let panel = CGRect(x: 32, y: 270, width: size.width - 64, height: 122)
    let shape = NSBezierPath(roundedRect: panel, xRadius: 14, yRadius: 14)
    NSColor.white.withAlphaComponent(0.05).setFill()
    shape.fill()
    NSColor.white.withAlphaComponent(0.09).setStroke()
    shape.lineWidth = 1
    shape.stroke()

    let inset = panel.minX + 20, width = panel.width - 40
    text("If macOS will not open Shepit", at: CGPoint(x: inset, y: panel.minY + 16),
         font: .systemFont(ofSize: 13, weight: .semibold), color: offWhite)
    text("Shepit is not notarized, so macOS blocks the first launch. Click Done in that message, "
         + "then open System Settings \u{2192} Privacy & Security, scroll down and click Open Anyway.",
         at: CGPoint(x: inset, y: panel.minY + 38), width: width,
         font: .systemFont(ofSize: 12.5), color: offWhite.withAlphaComponent(0.72))
    text("To skip this, install with one Terminal command from github.com/themarchenko/shepit",
         at: CGPoint(x: inset, y: panel.minY + 92), width: width,
         font: .systemFont(ofSize: 11.5), color: offWhite.withAlphaComponent(0.45))

    NSGraphicsContext.restoreGraphicsState()
    return rep
}

func text(_ string: String, at point: CGPoint, width: CGFloat = 600, font: NSFont, color: NSColor,
          centered: Bool = false) {
    let style = NSMutableParagraphStyle()
    style.alignment = centered ? .center : .left
    style.lineSpacing = 2
    let attributed = NSAttributedString(string: string, attributes: [
        .font: font, .foregroundColor: color, .paragraphStyle: style,
    ])
    let origin = centered ? CGPoint(x: point.x - width / 2, y: point.y) : point
    attributed.draw(with: CGRect(origin: origin, size: CGSize(width: width, height: 200)),
                    options: [.usesLineFragmentOrigin])
}

let root = URL(fileURLWithPath: CommandLine.arguments[0]).deletingLastPathComponent().deletingLastPathComponent()
let dmg = root.appendingPathComponent("dmg")
try FileManager.default.createDirectory(at: dmg, withIntermediateDirectories: true)
for (scale, name) in [(1.0, "background.png"), (2.0, "background@2x.png")] {
    let data = render(scale: scale).representation(using: .png, properties: [:])!
    try data.write(to: dmg.appendingPathComponent(name))
    print("wrote dmg/\(name)")
}
