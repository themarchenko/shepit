// Renders Shepit's app icon and menu bar glyph into Shepit/Assets.xcassets.
//
//   swift scripts/make-icons.swift [full|inset]
//
// The mark is "voice to cursor": three rising bars flowing into a text caret. Its geometry lives in a
// 100 x 100 box and matches `ShepitMark` in Shepit/Settings/Brand.swift; change both together.
// "full" draws edge to edge and lets macOS 26 apply its rounded mask; "inset" draws the pre-Tahoe
// squircle with margins and a shadow.
import AppKit
import CoreGraphics
import Foundation

struct Piece {
    let x, y, w, h: CGFloat
    var alpha: CGFloat = 1
}

/// The mark in the app icon and in the app's own UI.
let mark: [Piece] = [
    Piece(x: 18, y: 45, w: 5, h: 10, alpha: 0.35),
    Piece(x: 27, y: 40, w: 5, h: 20, alpha: 0.65),
    Piece(x: 36, y: 34.5, w: 5, h: 31, alpha: 0.95),
    Piece(x: 57, y: 24, w: 6, h: 52),
    Piece(x: 47, y: 21, w: 26, h: 6),
    Piece(x: 47, y: 73, w: 26, h: 6),
]

/// Heavier strokes and fewer bars so it holds up at 18 pt in the menu bar.
let glyph: [Piece] = [
    Piece(x: 12, y: 42, w: 10, h: 16, alpha: 0.5),
    Piece(x: 27, y: 33, w: 10, h: 34),
    Piece(x: 58, y: 16, w: 11, h: 68),
    Piece(x: 45, y: 11, w: 37, h: 11),
    Piece(x: 45, y: 78, w: 37, h: 11),
]

let root = URL(fileURLWithPath: CommandLine.arguments[0]).deletingLastPathComponent().deletingLastPathComponent()
let assets = root.appendingPathComponent("Shepit/Assets.xcassets")
let fullBleed = CommandLine.arguments.dropFirst().first != "inset"

func color(_ hex: UInt32, _ alpha: CGFloat = 1) -> CGColor {
    CGColor(srgbRed: CGFloat((hex >> 16) & 0xff) / 255, green: CGFloat((hex >> 8) & 0xff) / 255,
            blue: CGFloat(hex & 0xff) / 255, alpha: alpha)
}

func context(_ side: Int) -> CGContext {
    CGContext(data: nil, width: side, height: side, bitsPerComponent: 8, bytesPerRow: 0,
              space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
}

/// Draws pieces given in a top-left 100-unit box into `rect` (Core Graphics coordinates, y up).
func draw(_ pieces: [Piece], in rect: CGRect, color ink: CGColor, ctx: CGContext) {
    let s = rect.width / 100
    for p in pieces {
        let r = CGRect(x: rect.minX + p.x * s, y: rect.maxY - (p.y + p.h) * s, width: p.w * s, height: p.h * s)
        ctx.setFillColor(ink.copy(alpha: ink.alpha * p.alpha)!)
        ctx.addPath(CGPath(roundedRect: r, cornerWidth: min(r.width, r.height) / 2, cornerHeight: min(r.width, r.height) / 2, transform: nil))
        ctx.fillPath()
    }
}

func icon(_ side: Int) -> CGImage {
    let ctx = context(side)
    let full = CGFloat(side)
    // Apple's macOS grid: an 824-point squircle on a 1024 canvas.
    let tile = fullBleed ? CGRect(x: 0, y: 0, width: full, height: full)
        : CGRect(x: full * 100 / 1024, y: full * 100 / 1024 + full * 0.01, width: full * 824 / 1024, height: full * 824 / 1024)
    let radius = tile.width * 0.2237
    let shape = CGPath(roundedRect: tile, cornerWidth: radius, cornerHeight: radius, transform: nil)

    if !fullBleed {
        ctx.saveGState()
        ctx.setShadow(offset: CGSize(width: 0, height: -full * 0.01), blur: full * 0.03, color: color(0x000000, 0.45))
        ctx.addPath(shape)
        ctx.setFillColor(color(0x0b0b0d))
        ctx.fillPath()
        ctx.restoreGState()
    }

    ctx.saveGState()
    if !fullBleed { ctx.addPath(shape); ctx.clip() }
    let space = CGColorSpace(name: CGColorSpace.sRGB)!
    let base = CGGradient(colorsSpace: space, colors: [color(0x1b1c1f), color(0x08080a)] as CFArray, locations: [0, 1])!
    ctx.drawLinearGradient(base, start: CGPoint(x: tile.minX, y: tile.maxY), end: CGPoint(x: tile.maxX, y: tile.minY), options: [])
    // The teal and amber of the widget's preview wallpaper, as two soft glows.
    let teal = CGGradient(colorsSpace: space, colors: [color(0x5a6f70, 0.75), color(0x5a6f70, 0)] as CFArray, locations: [0, 1])!
    ctx.drawRadialGradient(teal, startCenter: CGPoint(x: tile.minX + tile.width * 0.18, y: tile.maxY - tile.height * 0.1), startRadius: 0,
                           endCenter: CGPoint(x: tile.minX + tile.width * 0.18, y: tile.maxY - tile.height * 0.1), endRadius: tile.width * 0.75, options: [])
    let amber = CGGradient(colorsSpace: space, colors: [color(0x7a6249, 0.7), color(0x7a6249, 0)] as CFArray, locations: [0, 1])!
    ctx.drawRadialGradient(amber, startCenter: CGPoint(x: tile.minX + tile.width * 0.9, y: tile.minY + tile.height * 0.02), startRadius: 0,
                           endCenter: CGPoint(x: tile.minX + tile.width * 0.9, y: tile.minY + tile.height * 0.02), endRadius: tile.width * 0.7, options: [])
    let markRect = tile.insetBy(dx: tile.width * 0.14, dy: tile.height * 0.14)
    draw(mark, in: markRect, color: color(0xf4f2ee), ctx: ctx)
    ctx.restoreGState()

    if !fullBleed {
        ctx.addPath(CGPath(roundedRect: tile.insetBy(dx: full * 0.002, dy: full * 0.002), cornerWidth: radius, cornerHeight: radius, transform: nil))
        ctx.setStrokeColor(color(0xffffff, 0.12))
        ctx.setLineWidth(max(1, full * 0.004))
        ctx.strokePath()
    }
    return ctx.makeImage()!
}

func glyphImage(_ side: Int) -> CGImage {
    let ctx = context(side)
    draw(glyph, in: CGRect(x: 0, y: 0, width: side, height: side), color: color(0x000000), ctx: ctx)
    return ctx.makeImage()!
}

func write(_ image: CGImage, to url: URL) throws {
    let rep = NSBitmapImageRep(cgImage: image)
    guard let data = rep.representation(using: .png, properties: [:]) else { throw CocoaError(.fileWriteUnknown) }
    try data.write(to: url)
}

func json(_ object: Any, to url: URL) throws {
    let data = try JSONSerialization.data(withJSONObject: object, options: [.prettyPrinted, .sortedKeys])
    try data.write(to: url)
}

let fm = FileManager.default
let iconSet = assets.appendingPathComponent("AppIcon.appiconset")
let glyphSet = assets.appendingPathComponent("MenuBarIcon.imageset")
try fm.createDirectory(at: iconSet, withIntermediateDirectories: true)
try fm.createDirectory(at: glyphSet, withIntermediateDirectories: true)
try json(["info": ["author": "xcode", "version": 1]], to: assets.appendingPathComponent("Contents.json"))

var images: [[String: String]] = []
for (points, scale) in [(16, 1), (16, 2), (32, 1), (32, 2), (128, 1), (128, 2), (256, 1), (256, 2), (512, 1), (512, 2)] {
    let pixels = points * scale
    let name = "icon_\(pixels).png"
    try write(icon(pixels), to: iconSet.appendingPathComponent(name))
    images.append(["idiom": "mac", "scale": "\(scale)x", "size": "\(points)x\(points)", "filename": name])
}
try json(["images": images, "info": ["author": "xcode", "version": 1]], to: iconSet.appendingPathComponent("Contents.json"))

try write(glyphImage(18), to: glyphSet.appendingPathComponent("menubar.png"))
try write(glyphImage(36), to: glyphSet.appendingPathComponent("menubar@2x.png"))
try json([
    "images": [
        ["idiom": "mac", "scale": "1x", "filename": "menubar.png"],
        ["idiom": "mac", "scale": "2x", "filename": "menubar@2x.png"],
    ],
    "info": ["author": "xcode", "version": 1],
    "properties": ["template-rendering-intent": "template"],
], to: glyphSet.appendingPathComponent("Contents.json"))

print("wrote \(iconSet.path) (\(fullBleed ? "full bleed" : "inset squircle")) and \(glyphSet.path)")
