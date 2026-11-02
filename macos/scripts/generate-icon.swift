// Native rendering of Kite's existing four-panel diamond and short white tail.
// Run: swift macos/scripts/generate-icon.swift
import AppKit
import Foundation

let root = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent()
let output = root.appendingPathComponent("Kite/Resources/Assets.xcassets/AppIcon.appiconset")
try FileManager.default.createDirectory(at: output, withIntermediateDirectories: true)
func rgb(_ r: CGFloat, _ g: CGFloat, _ b: CGFloat) -> NSColor { NSColor(srgbRed: r/255, green: g/255, blue: b/255, alpha: 1) }
func triangle(_ a: CGPoint, _ b: CGPoint, _ c: CGPoint, _ color: NSColor) {
    let p = NSBezierPath(); p.move(to: a); p.line(to: b); p.line(to: c); p.close(); color.setFill(); p.fill()
}
var images: [[String: String]] = []
for size in [16, 32, 128, 256, 512] {
    for scale in [1, 2] {
        let pixels = size * scale
        let bitmap = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: pixels, pixelsHigh: pixels, bitsPerSample: 8,
            samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: bitmap)
        let transform = NSAffineTransform(); transform.scale(by: CGFloat(pixels) / 1024); transform.concat()
        let tile = NSBezierPath(roundedRect: CGRect(x: 80, y: 80, width: 864, height: 864), xRadius: 188, yRadius: 188)
        NSGradient(starting: rgb(38, 51, 71), ending: rgb(15, 22, 35))!.draw(in: tile, angle: -70)
        let tail = NSBezierPath(); tail.move(to: CGPoint(x: 512, y: 280))
        tail.curve(to: CGPoint(x: 504, y: 162), controlPoint1: CGPoint(x: 444, y: 230), controlPoint2: CGPoint(x: 568, y: 209))
        tail.lineWidth = 19; tail.lineCapStyle = .round; rgb(250, 245, 233).setStroke(); tail.stroke()
        NSGraphicsContext.saveGraphicsState()
        let shadow = NSShadow(); shadow.shadowColor = NSColor.black.withAlphaComponent(0.3); shadow.shadowBlurRadius = 30; shadow.shadowOffset = CGSize(width: 0, height: -14); shadow.set()
        let outline = NSBezierPath(); outline.move(to: CGPoint(x: 512, y: 812)); outline.line(to: CGPoint(x: 776, y: 548)); outline.line(to: CGPoint(x: 512, y: 280)); outline.line(to: CGPoint(x: 248, y: 548)); outline.close()
        rgb(242, 169, 59).setFill(); outline.fill(); NSGraphicsContext.restoreGraphicsState()
        let top = CGPoint(x: 512, y: 812), right = CGPoint(x: 776, y: 548), bottom = CGPoint(x: 512, y: 280), left = CGPoint(x: 248, y: 548), center = CGPoint(x: 512, y: 548)
        triangle(top, left, center, rgb(248, 190, 80)); triangle(top, right, center, rgb(242, 169, 59))
        triangle(left, bottom, center, rgb(224, 122, 47)); triangle(right, bottom, center, rgb(205, 92, 39))
        NSGraphicsContext.restoreGraphicsState()
        let filename = "icon_\(size)x\(size)@\(scale)x.png"
        try bitmap.representation(using: .png, properties: [:])!.write(to: output.appendingPathComponent(filename))
        images.append(["idiom": "mac", "size": "\(size)x\(size)", "scale": "\(scale)x", "filename": filename])
    }
}
let catalog: [String: Any] = ["images": images, "info": ["version": 1, "author": "Kite"]]
try JSONSerialization.data(withJSONObject: catalog, options: [.prettyPrinted, .sortedKeys]).write(to: output.appendingPathComponent("Contents.json"))
