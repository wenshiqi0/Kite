import AppKit

struct VideoGeometry {
    var source: CGSize
    var bounds: CGSize
    var zoom: CGFloat = 1
    var pan: CGPoint = .zero

    var rect: CGRect {
        guard source.width > 0, source.height > 0 else { return .zero }
        let scale = min(bounds.width / source.width, bounds.height / source.height) * zoom
        let size = CGSize(width: source.width * scale, height: source.height * scale)
        return CGRect(x: (bounds.width - size.width) / 2 + pan.x,
                      y: (bounds.height - size.height) / 2 + pan.y, width: size.width, height: size.height)
    }
    func sourcePoint(_ point: CGPoint) -> CGPoint? {
        let r = rect
        guard r.width > 0, r.height > 0, r.contains(point) else { return nil }
        return CGPoint(x: min(source.width - 1, max(0, (point.x - r.minX) * source.width / r.width)),
                       y: min(source.height - 1, max(0, (point.y - r.minY) * source.height / r.height)))
    }
}

/// A fixed deadline is scheduled once; continuous motion cannot postpone it.
@MainActor
final class RemoteInput {
    var handle: UInt64 = 0
    var enabled = false { didSet { if !enabled { cancel() } } }
    private var position: (Int16, Int16, Int16, Int16)?
    private var wheel = CGPoint.zero
    private var committed = ""
    private var motionWork: DispatchWorkItem?
    private var textWork: DispatchWorkItem?

    func move(_ point: CGPoint, source: CGSize) {
        guard enabled, source.width > 0, source.height > 0, source.width <= 32767, source.height <= 32767 else { return }
        position = (Int16(clamping: Int(point.x)), Int16(clamping: Int(point.y)), Int16(source.width), Int16(source.height))
        scheduleMotion()
    }
    func scroll(horizontal: CGFloat, vertical: CGFloat) {
        guard enabled else { return }
        wheel.x += horizontal; wheel.y += vertical
        scheduleMotion()
    }
    private func scheduleMotion() {
        guard motionWork == nil else { return }
        let work = DispatchWorkItem { [weak self] in self?.flushMotion() }
        motionWork = work
        DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(16), execute: work)
    }
    private func flushMotion() {
        motionWork?.cancel(); motionWork = nil
        defer { position = nil; wheel = .zero }
        guard enabled else { return }
        if let (x, y, w, h) = position { _ = kite_apple_media_mouse_position(handle, x, y, w, h) }
        let x = Int16(clamping: Int(wheel.x.rounded())), y = Int16(clamping: Int(wheel.y.rounded()))
        if x != 0 || y != 0 { _ = kite_apple_media_scroll(handle, x, y) }
    }
    func text(_ text: String) {
        guard enabled, !text.isEmpty else { return }
        // Keep every Unicode scalar intact across the media ABI's 4 KiB bound.
        for scalar in text.unicodeScalars {
            if committed.utf8.count + scalar.utf8.count > 4096 { flushText() }
            committed.unicodeScalars.append(scalar)
        }
        guard textWork == nil else { return }
        let work = DispatchWorkItem { [weak self] in self?.flushText() }
        textWork = work
        DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(200), execute: work)
    }
    private func flushText() {
        textWork?.cancel(); textWork = nil
        defer { committed = "" }
        guard enabled, !committed.isEmpty else { return }
        Array(committed.utf8).withUnsafeBufferPointer { bytes in
            _ = kite_apple_media_text(handle, bytes.baseAddress, UInt32(bytes.count))
        }
    }
    func barrier() { flushText(); flushMotion() }
    func button(_ button: UInt8, down: Bool) {
        guard enabled else { return }
        barrier(); _ = kite_apple_media_mouse_button(handle, button, down ? 1 : 0)
    }
    func key(_ key: Int16, down: Bool, modifiers: UInt8) {
        guard enabled else { return }
        barrier(); _ = kite_apple_media_key(handle, key, down ? 1 : 0, modifiers)
    }
    func cancel() {
        motionWork?.cancel(); textWork?.cancel()
        motionWork = nil; textWork = nil; position = nil; wheel = .zero; committed = ""
        if handle != 0 { kite_apple_media_release_input(handle) }
    }
}

enum RemoteKeys {
    // macOS virtual key codes to Moonlight's Windows virtual keys. Printable
    // text goes through NSTextInputClient; only shortcuts send physical keys.
    static let map: [UInt16: Int16] = [
        0: 0x41, 1: 0x53, 2: 0x44, 3: 0x46, 4: 0x48, 5: 0x47,
        6: 0x5A, 7: 0x58, 8: 0x43, 9: 0x56, 11: 0x42, 12: 0x51,
        13: 0x57, 14: 0x45, 15: 0x52, 16: 0x59, 17: 0x54,
        18: 0x31, 19: 0x32, 20: 0x33, 21: 0x34, 22: 0x36, 23: 0x35,
        24: 0xBB, 25: 0x39, 26: 0x37, 27: 0xBD, 28: 0x38, 29: 0x30,
        30: 0xDD, 31: 0x4F, 32: 0x55, 33: 0xDB, 34: 0x49, 35: 0x50,
        36: 0x0D, 37: 0x4C, 38: 0x4A, 39: 0xDE, 40: 0x4B, 41: 0xBA,
        42: 0xDC, 43: 0xBC, 44: 0xBF, 45: 0x4E, 46: 0x4D, 47: 0xBE,
        48: 0x09, 49: 0x20, 50: 0xC0, 51: 0x08, 53: 0x1B,
        55: 0x5B, 54: 0x5C, 56: 0xA0, 60: 0xA1, 59: 0xA2, 62: 0xA3, 58: 0xA4, 61: 0xA5,
        65: 0x6E, 67: 0x6A, 69: 0x6B, 75: 0x6F, 76: 0x0D, 78: 0x6D,
        82: 0x60, 83: 0x61, 84: 0x62, 85: 0x63, 86: 0x64, 87: 0x65,
        88: 0x66, 89: 0x67, 91: 0x68, 92: 0x69,
        122: 0x70, 120: 0x71, 99: 0x72, 118: 0x73, 96: 0x74, 97: 0x75,
        98: 0x76, 100: 0x77, 101: 0x78, 109: 0x79, 103: 0x7A, 111: 0x7B,
        114: 0x2D, 115: 0x24, 116: 0x21, 117: 0x2E, 119: 0x23, 121: 0x22,
        123: 0x25, 124: 0x27, 125: 0x28, 126: 0x26
    ]
    static let special: Set<UInt16> = [36, 48, 51, 53, 76, 114, 115, 116, 117, 119, 121, 123, 124, 125, 126, 122, 120, 99, 118, 96, 97, 98, 100, 101, 109, 103, 111]
    static func modifiers(_ flags: NSEvent.ModifierFlags) -> UInt8 {
        (flags.contains(.shift) ? 1 : 0) | (flags.contains(.control) ? 2 : 0) |
        (flags.contains(.option) ? 4 : 0) | (flags.contains(.command) ? 8 : 0)
    }
}
