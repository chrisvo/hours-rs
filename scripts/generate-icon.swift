import AppKit

let size = 1024
let image = NSImage(size: NSSize(width: size, height: size))
image.lockFocus()
NSColor(calibratedRed: 0.24, green: 0.17, blue: 0.14, alpha: 1).setFill()
NSBezierPath(roundedRect: NSRect(x: 40, y: 40, width: 944, height: 944), xRadius: 208, yRadius: 208).fill()
NSColor(calibratedRed: 0.78, green: 0.65, blue: 0.43, alpha: 1).setStroke()
let border = NSBezierPath(roundedRect: NSRect(x: 79, y: 79, width: 866, height: 866), xRadius: 174, yRadius: 174)
border.lineWidth = 3
border.stroke()
let circle = NSBezierPath(ovalIn: NSRect(x: 235, y: 235, width: 554, height: 554))
circle.lineWidth = 3
circle.stroke()
let cross = NSBezierPath()
cross.move(to: NSPoint(x: 512, y: 327))
cross.line(to: NSPoint(x: 512, y: 728))
cross.move(to: NSPoint(x: 393, y: 590))
cross.line(to: NSPoint(x: 631, y: 590))
cross.lineWidth = 29
cross.lineCapStyle = .square
cross.stroke()
for i in 0..<12 {
    let angle = Double(i) * .pi / 6
    let ray = NSBezierPath()
    ray.move(to: NSPoint(x: 512 + cos(angle) * 300, y: 512 + sin(angle) * 300))
    ray.line(to: NSPoint(x: 512 + cos(angle) * 328, y: 512 + sin(angle) * 328))
    ray.lineWidth = 4
    ray.stroke()
}
image.unlockFocus()
let bitmap = NSBitmapImageRep(data: image.tiffRepresentation!)!
try bitmap.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: CommandLine.arguments[1]))
