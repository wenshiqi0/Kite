import AppKit
import CoreImage
import MetalKit
import SwiftUI

final class FramePresenter: @unchecked Sendable {
    private let lock = NSLock()
    private var latest: (CVPixelBuffer, UInt64, UInt64)?
    private var sequence: UInt64 = 0
    private var scheduled = false
    @MainActor weak var view: VideoCanvas?
    func offer(_ buffer: CVPixelBuffer, generation: UInt64) {
        lock.lock(); sequence &+= 1; latest = (buffer, generation, sequence)
        let schedule = !scheduled; scheduled = true; lock.unlock()
        if schedule { DispatchQueue.main.async { [weak self] in self?.view?.needsDisplay = true } }
    }
    func take() -> (CVPixelBuffer, UInt64, UInt64)? {
        lock.lock(); defer { lock.unlock() }
        scheduled = false
        return latest
    }
    func clear() {
        lock.lock(); latest = nil; scheduled = false; lock.unlock()
        DispatchQueue.main.async { [weak self] in self?.view?.needsDisplay = true }
    }
}

final class MediaReceiver: @unchecked Sendable {
    let decoder = VideoDecoder()
    let audio = AudioPlayer()
    let presenter = FramePresenter()
    @MainActor let input = RemoteInput()
    @MainActor var leaveOperation: (() -> Void)?
    private(set) var handle: UInt64 = 0
    private var attached = false

    @MainActor init() {
        decoder.output = { [weak self] buffer, generation in self?.presenter.offer(buffer, generation: generation) }
        decoder.failure = { [weak self] error, generation in
            guard let self else { return }
            kite_apple_media_decoder_failed(handle, generation, error)
        }
    }
    @MainActor func attach() {
        guard !attached else { return }
        var callbacks = KiteAppleMediaCallbacks(
            release: { context in if let context { Unmanaged<MediaReceiver>.fromOpaque(context).release() } },
            supports: { _, width, height, fps in (width > 0 && height > 0 && width <= 8192 && height <= 8192 && fps <= 120) ? 1 : 0 },
            configure: { context, _, _, _, generation in
                guard let context else { return 0 }
                Unmanaged<MediaReceiver>.fromOpaque(context).takeUnretainedValue().decoder.configure(generation: generation)
                return 1
            },
            frame: { context, bytes, length, pts, flags, generation in
                guard let context, let bytes else { return 0 }
                return Unmanaged<MediaReceiver>.fromOpaque(context).takeUnretainedValue().decoder.submit(bytes, count: Int(length), pts: pts, flags: flags, generation: generation) ? 1 : 0
            },
            audio_configure: { context, rate, channels, samples, generation in
                guard let context else { return }
                Unmanaged<MediaReceiver>.fromOpaque(context).takeUnretainedValue().audio.configure(rate: rate, channels: channels, samples: samples, generation: generation)
            },
            audio_frame: { context, bytes, length, generation in
                guard let context, let bytes else { return }
                Unmanaged<MediaReceiver>.fromOpaque(context).takeUnretainedValue().audio.submit(bytes, count: Int(length), generation: generation)
            },
            stopped: { context, generation in
                guard let context else { return }
                let receiver = Unmanaged<MediaReceiver>.fromOpaque(context).takeUnretainedValue()
                receiver.decoder.stop(generation: generation); receiver.audio.stop(generation: generation); receiver.presenter.clear()
            })
        let owner = Unmanaged.passRetained(self)
        handle = kite_apple_media_register(owner.toOpaque(), &callbacks)
        if handle == 0 { owner.release() }
        else { attached = true; input.handle = handle }
    }
    @MainActor func detach() {
        input.enabled = false; input.cancel()
        guard attached else { return }
        attached = false
        kite_apple_media_detach(handle)
    }
}

struct DesktopSurface: NSViewRepresentable {
    let receiver: MediaReceiver
    let source: CGSize
    let zoom: CGFloat
    let takeover: Bool
    func makeNSView(context: Context) -> VideoCanvas {
        let view = VideoCanvas(receiver: receiver)
        receiver.presenter.view = view
        return view
    }
    func updateNSView(_ view: VideoCanvas, context: Context) {
        view.source = source; view.zoom = zoom; view.takeover = takeover
        view.needsDisplay = true
    }
    static func dismantleNSView(_ view: VideoCanvas, coordinator: ()) {
        view.receiver.input.cancel()
        if view.receiver.presenter.view === view { view.receiver.presenter.view = nil }
    }
}

@MainActor
final class VideoCanvas: MTKView, MTKViewDelegate, @preconcurrency NSTextInputClient {
    let receiver: MediaReceiver
    var source = CGSize(width: 1920, height: 1080)
    var zoom: CGFloat = 1 { didSet { if zoom == 1 { pan = .zero } } }
    var takeover = false { didSet { if !takeover { resetInput() } } }
    private var pan = CGPoint.zero
    private var tracking: NSTrackingArea?
    private var marked = NSAttributedString(string: "")
    private var physical: Set<UInt16> = []
    private var modifierFlags: NSEvent.ModifierFlags = []
    private var lastPresented: UInt64 = 0
    private let commands: MTLCommandQueue?
    private let ci: CIContext?
    private var resignObserver: NSObjectProtocol?
    override var isFlipped: Bool { true }
    override var acceptsFirstResponder: Bool { takeover }
    var geometry: VideoGeometry { VideoGeometry(source: source, bounds: bounds.size, zoom: zoom, pan: pan) }

    init(receiver: MediaReceiver) {
        self.receiver = receiver
        let device = MTLCreateSystemDefaultDevice()
        commands = device?.makeCommandQueue()
        ci = device.map { CIContext(mtlDevice: $0) }
        super.init(frame: .zero, device: device)
        delegate = self; isPaused = true; enableSetNeedsDisplay = true
        framebufferOnly = false; autoResizeDrawable = true
        clearColor = MTLClearColorMake(0.015, 0.022, 0.035, 1)
        setAccessibilityLabel(tr("desktop_surface"))
    }
    required init(coder: NSCoder) { fatalError("init(coder:) is unavailable") }
    deinit { if let resignObserver { NotificationCenter.default.removeObserver(resignObserver) } }
    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        if let resignObserver { NotificationCenter.default.removeObserver(resignObserver) }
        resignObserver = nil
        if let window {
            resignObserver = NotificationCenter.default.addObserver(forName: NSWindow.didResignKeyNotification, object: window, queue: .main) { [weak self] _ in
                MainActor.assumeIsolated { self?.resetInput() }
            }
        } else { resetInput() }
    }
    override func updateTrackingAreas() {
        if let tracking { removeTrackingArea(tracking) }
        tracking = NSTrackingArea(rect: .zero, options: [.mouseMoved, .activeInKeyWindow, .inVisibleRect], owner: self)
        addTrackingArea(tracking!)
        super.updateTrackingAreas()
    }
    func mtkView(_ view: MTKView, drawableSizeWillChange size: CGSize) {}
    func draw(in view: MTKView) {
        guard let drawable = currentDrawable, let command = commands?.makeCommandBuffer() else { return }
        if let (buffer, generation, sequence) = receiver.presenter.take(), let ci {
            let image = CIImage(cvPixelBuffer: buffer)
            let sourceSize = CGSize(width: CVPixelBufferGetWidth(buffer), height: CVPixelBufferGetHeight(buffer))
            let rect = VideoGeometry(source: sourceSize, bounds: drawableSize, zoom: zoom,
                pan: CGPoint(x: pan.x * drawableSize.width / max(1, bounds.width), y: pan.y * drawableSize.height / max(1, bounds.height))).rect
            let scaled = image.transformed(by: CGAffineTransform(scaleX: rect.width / sourceSize.width, y: rect.height / sourceSize.height))
                .transformed(by: CGAffineTransform(translationX: rect.minX, y: drawableSize.height - rect.maxY))
            let full = scaled.composited(over: CIImage(color: .black).cropped(to: CGRect(origin: .zero, size: drawableSize)))
            ci.render(full, to: drawable.texture, commandBuffer: command, bounds: CGRect(origin: .zero, size: drawableSize), colorSpace: CGColorSpaceCreateDeviceRGB())
            let handle = receiver.handle
            if sequence != lastPresented {
                lastPresented = sequence
                drawable.addPresentedHandler { _ in kite_apple_media_frame_rendered(handle, generation) }
            }
        } else if let pass = currentRenderPassDescriptor { command.makeRenderCommandEncoder(descriptor: pass)?.endEncoding() }
        command.present(drawable); command.commit()
    }
    private func move(_ event: NSEvent) -> Bool {
        guard takeover, let point = geometry.sourcePoint(convert(event.locationInWindow, from: nil)) else { return false }
        receiver.input.move(point, source: source); return true
    }
    override func mouseMoved(with event: NSEvent) { _ = move(event) }
    override func mouseDown(with event: NSEvent) {
        if takeover, move(event) { window?.makeFirstResponder(self); receiver.input.button(1, down: true) }
    }
    override func mouseUp(with event: NSEvent) { if takeover { _ = move(event); receiver.input.button(1, down: false) } }
    override func rightMouseDown(with event: NSEvent) { if move(event) { window?.makeFirstResponder(self); receiver.input.button(3, down: true) } }
    override func rightMouseUp(with event: NSEvent) { if takeover { _ = move(event); receiver.input.button(3, down: false) } }
    override func otherMouseDown(with event: NSEvent) { if move(event) { receiver.input.button(2, down: true) } }
    override func otherMouseUp(with event: NSEvent) { if takeover { _ = move(event); receiver.input.button(2, down: false) } }
    override func mouseDragged(with event: NSEvent) {
        if takeover { _ = move(event) }
        else if zoom > 1 { pan.x += event.deltaX; pan.y += event.deltaY; needsDisplay = true }
    }
    override func rightMouseDragged(with event: NSEvent) { _ = move(event) }
    override func otherMouseDragged(with event: NSEvent) { _ = move(event) }
    override func scrollWheel(with event: NSEvent) {
        if takeover { receiver.input.scroll(horizontal: event.scrollingDeltaX * 6, vertical: event.scrollingDeltaY * 6) }
        else if zoom > 1 { pan.x += event.scrollingDeltaX; pan.y += event.scrollingDeltaY; needsDisplay = true }
    }
    override func keyDown(with event: NSEvent) {
        guard takeover else { return }
        if event.modifierFlags.intersection([.command, .control]).isEmpty && !RemoteKeys.special.contains(event.keyCode) {
            interpretKeyEvents([event])
        } else if let key = RemoteKeys.map[event.keyCode] {
            physical.insert(event.keyCode)
            receiver.input.key(key, down: true, modifiers: RemoteKeys.modifiers(event.modifierFlags))
        }
    }
    override func keyUp(with event: NSEvent) {
        if physical.remove(event.keyCode) != nil, let key = RemoteKeys.map[event.keyCode] {
            receiver.input.key(key, down: false, modifiers: RemoteKeys.modifiers(event.modifierFlags))
        }
    }
    override func flagsChanged(with event: NSEvent) {
        guard takeover, let key = RemoteKeys.map[event.keyCode] else { return }
        let flag: NSEvent.ModifierFlags
        switch event.keyCode {
        case 54, 55: flag = .command
        case 56, 60: flag = .shift
        case 58, 61: flag = .option
        case 59, 62: flag = .control
        default: return
        }
        let down = event.modifierFlags.contains(flag)
        modifierFlags = event.modifierFlags
        receiver.input.key(key, down: down, modifiers: RemoteKeys.modifiers(event.modifierFlags))
    }
    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        guard takeover, window?.firstResponder === self else { return super.performKeyEquivalent(with: event) }
        // A local escape hatch always stays available.
        if event.modifierFlags.contains([.command, .option]), event.keyCode == 53 {
            resetInput(); receiver.leaveOperation?(); return true
        }
        keyDown(with: event); return true
    }
    override func resignFirstResponder() -> Bool { resetInput(); return super.resignFirstResponder() }
    private func resetInput() {
        receiver.input.cancel(); physical.removeAll(); modifierFlags = []
        marked = NSAttributedString(string: ""); inputContext?.discardMarkedText()
    }
    func insertText(_ string: Any, replacementRange: NSRange) {
        guard takeover else { return }
        let value = (string as? NSAttributedString)?.string ?? (string as? String ?? "")
        marked = NSAttributedString(string: ""); receiver.input.text(value)
    }
    func setMarkedText(_ string: Any, selectedRange: NSRange, replacementRange: NSRange) {
        marked = (string as? NSAttributedString) ?? NSAttributedString(string: string as? String ?? "")
    }
    func unmarkText() { marked = NSAttributedString(string: "") }
    func selectedRange() -> NSRange { NSRange(location: NSNotFound, length: 0) }
    func markedRange() -> NSRange { hasMarkedText() ? NSRange(location: 0, length: marked.length) : NSRange(location: NSNotFound, length: 0) }
    func hasMarkedText() -> Bool { marked.length > 0 }
    func attributedSubstring(forProposedRange range: NSRange, actualRange: NSRangePointer?) -> NSAttributedString? { nil }
    func validAttributesForMarkedText() -> [NSAttributedString.Key] { [] }
    func firstRect(forCharacterRange range: NSRange, actualRange: NSRangePointer?) -> NSRect {
        window?.convertToScreen(convert(CGRect(x: bounds.midX, y: bounds.midY, width: 1, height: 20), to: nil)) ?? .zero
    }
    func characterIndex(for point: NSPoint) -> Int { NSNotFound }
    override func doCommand(by selector: Selector) {
        let key: Int16?
        switch selector {
        case #selector(insertNewline(_:)): key = 0x0D
        case #selector(deleteBackward(_:)): key = 0x08
        case #selector(insertTab(_:)): key = 0x09
        case #selector(cancelOperation(_:)): key = 0x1B
        default: key = nil
        }
        if let key { receiver.input.key(key, down: true, modifiers: 0); receiver.input.key(key, down: false, modifiers: 0) }
    }
}
