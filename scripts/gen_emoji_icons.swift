#!/usr/bin/env swift
import AppKit
import Foundation

/// Render the brand emoji into the app-icon source PNG.
/// Run from repo root, then: npx tauri icon src-tauri/icons/app-icon-source.png
let emoji = "🤫"
let size = 1024
let yellow = NSColor(srgbRed: 1, green: 210 / 255, blue: 61 / 255, alpha: 1) // #ffd23d

let repoRoot = URL(fileURLWithPath: CommandLine.arguments[1])
let out = repoRoot.appendingPathComponent("src-tauri/icons/app-icon-source.png")

guard let rep = NSBitmapImageRep(
    bitmapDataPlanes: nil,
    pixelsWide: size,
    pixelsHigh: size,
    bitsPerSample: 8,
    samplesPerPixel: 4,
    hasAlpha: true,
    isPlanar: false,
    colorSpaceName: .deviceRGB,
    bytesPerRow: 0,
    bitsPerPixel: 0
) else {
    fputs("failed to create bitmap\n", stderr)
    exit(1)
}
rep.size = NSSize(width: size, height: size)

NSGraphicsContext.saveGraphicsState()
guard let ctx = NSGraphicsContext(bitmapImageRep: rep) else {
    fputs("failed to create graphics context\n", stderr)
    exit(1)
}
NSGraphicsContext.current = ctx

yellow.setFill()
NSRect(x: 0, y: 0, width: size, height: size).fill()

let font = NSFont.systemFont(ofSize: CGFloat(size) * 0.62)
let para = NSMutableParagraphStyle()
para.alignment = .center
let attrs: [NSAttributedString.Key: Any] = [
    .font: font,
    .paragraphStyle: para,
]
let str = NSAttributedString(string: emoji, attributes: attrs)
let textSize = str.size()
let rect = NSRect(
    x: (CGFloat(size) - textSize.width) / 2,
    y: (CGFloat(size) - textSize.height) / 2 - CGFloat(size) * 0.02,
    width: textSize.width,
    height: textSize.height
)
str.draw(in: rect)

NSGraphicsContext.restoreGraphicsState()

guard let png = rep.representation(using: .png, properties: [:]) else {
    fputs("failed to encode png\n", stderr)
    exit(1)
}
try FileManager.default.createDirectory(
    at: out.deletingLastPathComponent(),
    withIntermediateDirectories: true
)
try png.write(to: out)
print("wrote \(out.path) (\(size)x\(size))")
