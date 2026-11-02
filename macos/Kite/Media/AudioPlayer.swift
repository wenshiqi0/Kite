import AVFoundation

/// Bounded Opus packet queue. Audio work never runs on the video or UI queues.
final class AudioPlayer: @unchecked Sendable {
    private let queue = DispatchQueue(label: "kite.mac.audio", qos: .userInteractive)
    private let slots = DispatchSemaphore(value: 12)
    private let lock = NSLock()
    private var generation: UInt64 = 0
    private var mute = false
    private var pending = 0
    private var engine: AVAudioEngine?
    private var player: AVAudioPlayerNode?
    private var converter: AVAudioConverter?
    private var source: AVAudioFormat?
    private var pcm: AVAudioFormat?
    private var samples: AVAudioFrameCount = 480
    var muted: Bool {
        get { lock.lock(); defer { lock.unlock() }; return mute }
        set { lock.lock(); mute = newValue; lock.unlock(); queue.async { [self] in player?.volume = newValue ? 0 : 1 } }
    }
    func configure(rate: UInt32, channels: UInt32, samples: UInt32, generation: UInt64) {
        lock.lock(); self.generation = generation; lock.unlock()
        queue.async { [self] in
            reset()
            guard rate == 48000, channels == 2, (120...2880).contains(samples) else { return }
            var description = AudioStreamBasicDescription(mSampleRate: Double(rate), mFormatID: kAudioFormatOpus,
                mFormatFlags: 0, mBytesPerPacket: 0, mFramesPerPacket: samples, mBytesPerFrame: 0, mChannelsPerFrame: channels, mBitsPerChannel: 0, mReserved: 0)
            guard let source = AVAudioFormat(streamDescription: &description),
                  let pcm = AVAudioFormat(standardFormatWithSampleRate: Double(rate), channels: channels),
                  let converter = AVAudioConverter(from: source, to: pcm) else { return }
            self.source = source; self.pcm = pcm; self.converter = converter; self.samples = samples
            let engine = AVAudioEngine(), player = AVAudioPlayerNode()
            engine.attach(player); engine.connect(player, to: engine.mainMixerNode, format: pcm)
            do { try engine.start(); player.volume = muted ? 0 : 1; player.play(); self.engine = engine; self.player = player }
            catch { reset() }
        }
    }
    func submit(_ bytes: UnsafePointer<UInt8>, count: Int, generation: UInt64) {
        guard count > 0, count <= 4096, slots.wait(timeout: .now()) == .success else { return }
        let data = Data(bytes: bytes, count: count) // At most 4 KiB, independent of video slots.
        queue.async { [self] in
            defer { slots.signal() }
            lock.lock(); let current = self.generation == generation; lock.unlock()
            guard current, let converter, let source, let pcm, let player, pending < 12 else { return }
            let input = AVAudioCompressedBuffer(format: source, packetCapacity: 1, maximumPacketSize: 4096)
            input.packetCount = 1; input.byteLength = UInt32(data.count)
            data.copyBytes(to: input.data.assumingMemoryBound(to: UInt8.self), count: data.count)
            input.packetDescriptions?.pointee = AudioStreamPacketDescription(mStartOffset: 0, mVariableFramesInPacket: samples, mDataByteSize: UInt32(data.count))
            guard let output = AVAudioPCMBuffer(pcmFormat: pcm, frameCapacity: samples * 2) else { return }
            var supplied = false, error: NSError?
            let result = converter.convert(to: output, error: &error) { _, status in
                if supplied { status.pointee = .noDataNow; return nil }
                supplied = true; status.pointee = .haveData; return input
            }
            guard result != .error, output.frameLength > 0 else { return }
            pending += 1
            player.scheduleBuffer(output, completionCallbackType: .dataPlayedBack) { [weak self] _ in
                self?.queue.async { [weak self] in
                    guard let self else { return }
                    self.lock.lock(); let current = self.generation == generation; self.lock.unlock()
                    if current { self.pending = max(0, self.pending - 1) }
                }
            }
        }
    }
    func stop(generation: UInt64) {
        lock.lock(); self.generation = generation; lock.unlock()
        queue.async { [self] in reset() }
    }
    private func reset() {
        player?.stop(); engine?.stop(); player = nil; engine = nil
        converter = nil; source = nil; pcm = nil; pending = 0
    }
}
