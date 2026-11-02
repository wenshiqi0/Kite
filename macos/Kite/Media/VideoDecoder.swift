import Foundation
import VideoToolbox

enum AnnexB {
    static func ranges(_ bytes: UnsafeBufferPointer<UInt8>) -> [Range<Int>] {
        var starts: [(Int, Int)] = [], i = 0
        while i + 2 < bytes.count {
            if bytes[i] == 0 && bytes[i + 1] == 0 {
                if bytes[i + 2] == 1 { starts.append((i, 3)); i += 3; continue }
                if i + 3 < bytes.count && bytes[i + 2] == 0 && bytes[i + 3] == 1 {
                    starts.append((i, 4)); i += 4; continue
                }
            }
            i += 1
        }
        return starts.enumerated().compactMap { index, start in
            let lower = start.0 + start.1, upper = index + 1 < starts.count ? starts[index + 1].0 : bytes.count
            return lower < upper ? lower..<upper : nil
        }
    }
}

/// Only the small slot bookkeeping lock runs on a Moonlight callback. All
/// format work, CM allocation and decode submission happen on the decoder queue.
final class VideoDecoder: @unchecked Sendable {
    private let queue = DispatchQueue(label: "kite.mac.video", qos: .userInteractive)
    private let lock = NSLock()
    private let capacity = 8 * 1024 * 1024
    private let slots = (0..<4).map { _ in UnsafeMutablePointer<UInt8>.allocate(capacity: 8 * 1024 * 1024) }
    private var available = [true, true, true, true]
    private var generation: UInt64 = 0
    private var active = false
    // Decoder queue only.
    private var session: VTDecompressionSession?
    private var format: CMVideoFormatDescription?
    private var sps = Data(), pps = Data()
    var output: (@Sendable (CVPixelBuffer, UInt64) -> Void)?
    var failure: (@Sendable (Int32, UInt64) -> Void)?

    deinit {
        if let session { VTDecompressionSessionWaitForAsynchronousFrames(session); VTDecompressionSessionInvalidate(session) }
        slots.forEach { $0.deallocate() }
    }
    func configure(generation: UInt64) {
        lock.lock(); self.generation = generation; active = true; lock.unlock()
        queue.async { [self] in reset() }
    }
    func stop(generation: UInt64) {
        lock.lock(); self.generation = generation; active = false; lock.unlock()
        queue.async { [self] in reset() }
    }
    private func reset() {
        if let session { VTDecompressionSessionWaitForAsynchronousFrames(session); VTDecompressionSessionInvalidate(session) }
        session = nil; format = nil; sps.removeAll(); pps.removeAll()
    }
    private func current(_ generation: UInt64) -> Bool {
        lock.lock(); defer { lock.unlock() }
        return active && self.generation == generation
    }
    func submit(_ bytes: UnsafePointer<UInt8>, count: Int, pts: UInt64, flags: UInt32, generation: UInt64) -> Bool {
        guard count > 0, count <= capacity else { return false }
        lock.lock()
        guard active, self.generation == generation, let index = available.firstIndex(of: true) else { lock.unlock(); return false }
        available[index] = false
        lock.unlock()
        slots[index].update(from: bytes, count: count)
        queue.async { [self] in
            defer { lock.lock(); available[index] = true; lock.unlock() }
            guard current(generation) else { return }
            decode(UnsafeBufferPointer(start: slots[index], count: count), pts: pts, flags: flags, generation: generation)
        }
        return true
    }
    private func decode(_ bytes: UnsafeBufferPointer<UInt8>, pts: UInt64, flags: UInt32, generation: UInt64) {
        let ranges = AnnexB.ranges(bytes)
        guard !ranges.isEmpty, let base = bytes.baseAddress else { failure?(-1, generation); return }
        if flags & 1 != 0 {
            for r in ranges {
                switch bytes[r.lowerBound] & 31 {
                case 7: sps = Data(bytes: base + r.lowerBound, count: r.count)
                case 8: pps = Data(bytes: base + r.lowerBound, count: r.count)
                default: break
                }
            }
            return
        }
        if session == nil {
            guard !sps.isEmpty, !pps.isEmpty else { return }
            let status = sps.withUnsafeBytes { s in pps.withUnsafeBytes { p -> OSStatus in
                var pointers = [s.baseAddress!.assumingMemoryBound(to: UInt8.self), p.baseAddress!.assumingMemoryBound(to: UInt8.self)]
                var sizes = [s.count, p.count]
                return CMVideoFormatDescriptionCreateFromH264ParameterSets(allocator: kCFAllocatorDefault, parameterSetCount: 2,
                    parameterSetPointers: &pointers, parameterSetSizes: &sizes, nalUnitHeaderLength: 4, formatDescriptionOut: &format)
            } }
            guard status == noErr, let format else { failure?(status, generation); return }
            var callback = VTDecompressionOutputCallbackRecord(decompressionOutputCallback: { context, frame, status, _, image, _, _ in
                guard let context else { return }
                let decoder = Unmanaged<VideoDecoder>.fromOpaque(context).takeUnretainedValue()
                let generation = UInt64(UInt(bitPattern: frame))
                guard decoder.current(generation) else { return }
                if status != noErr { decoder.failure?(status, generation) }
                else if let image { decoder.output?(image, generation) }
            }, decompressionOutputRefCon: Unmanaged.passUnretained(self).toOpaque())
            let spec = [kVTVideoDecoderSpecification_EnableHardwareAcceleratedVideoDecoder: true] as CFDictionary
            let attrs = [kCVPixelBufferPixelFormatTypeKey: kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
                         kCVPixelBufferMetalCompatibilityKey: true] as CFDictionary
            let result = VTDecompressionSessionCreate(allocator: kCFAllocatorDefault, formatDescription: format,
                decoderSpecification: spec, imageBufferAttributes: attrs, outputCallback: &callback, decompressionSessionOut: &session)
            guard result == noErr else { failure?(result, generation); return }
            if let session { VTSessionSetProperty(session, key: kVTDecompressionPropertyKey_RealTime, value: kCFBooleanTrue) }
        }
        guard let session, let format else { return }
        let size = ranges.reduce(0) { $0 + 4 + $1.count }
        var block: CMBlockBuffer?
        var status = CMBlockBufferCreateWithMemoryBlock(allocator: kCFAllocatorDefault, memoryBlock: nil, blockLength: size,
            blockAllocator: kCFAllocatorDefault, customBlockSource: nil, offsetToData: 0, dataLength: size, flags: 0, blockBufferOut: &block)
        guard status == noErr, let block else { failure?(status, generation); return }
        var offset = 0
        for r in ranges {
            var length = UInt32(r.count).bigEndian
            status = CMBlockBufferReplaceDataBytes(with: &length, blockBuffer: block, offsetIntoDestination: offset, dataLength: 4)
            guard status == noErr else { failure?(status, generation); return }
            status = CMBlockBufferReplaceDataBytes(with: base + r.lowerBound, blockBuffer: block, offsetIntoDestination: offset + 4, dataLength: r.count)
            guard status == noErr else { failure?(status, generation); return }
            offset += 4 + r.count
        }
        var timing = CMSampleTimingInfo(duration: .invalid, presentationTimeStamp: CMTime(value: Int64(clamping: pts), timescale: 1_000_000), decodeTimeStamp: .invalid)
        var sample: CMSampleBuffer?, sampleSize = size
        status = CMSampleBufferCreateReady(allocator: kCFAllocatorDefault, dataBuffer: block, formatDescription: format,
            sampleCount: 1, sampleTimingEntryCount: 1, sampleTimingArray: &timing, sampleSizeEntryCount: 1, sampleSizeArray: &sampleSize, sampleBufferOut: &sample)
        guard status == noErr, let sample else { failure?(status, generation); return }
        status = VTDecompressionSessionDecodeFrame(session, sampleBuffer: sample,
            flags: [._EnableAsynchronousDecompression, ._EnableTemporalProcessing],
            frameRefcon: UnsafeMutableRawPointer(bitPattern: UInt(generation)), infoFlagsOut: nil)
        if status != noErr { failure?(status, generation) }
    }
}
