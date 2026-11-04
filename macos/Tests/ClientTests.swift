import XCTest
import AVFoundation
@testable import Kite

final class ClientTests: XCTestCase {
    func testNativeOpusPacketDecode() throws {
        var description = AudioStreamBasicDescription(mSampleRate: 48000, mFormatID: kAudioFormatOpus,
            mFormatFlags: 0, mBytesPerPacket: 0, mFramesPerPacket: 480, mBytesPerFrame: 0, mChannelsPerFrame: 2, mBitsPerChannel: 0, mReserved: 0)
        let source = try XCTUnwrap(AVAudioFormat(streamDescription: &description))
        let pcm = try XCTUnwrap(AVAudioFormat(standardFormatWithSampleRate: 48000, channels: 2))
        let converter = try XCTUnwrap(AVAudioConverter(from: source, to: pcm))
        let url = try XCTUnwrap(Bundle(for: Self.self).url(forResource: "silence", withExtension: "opuspacket"))
        let packet = try Data(contentsOf: url)
        let input = AVAudioCompressedBuffer(format: source, packetCapacity: 1, maximumPacketSize: 4096)
        input.packetCount = 1; input.byteLength = UInt32(packet.count)
        packet.copyBytes(to: input.data.assumingMemoryBound(to: UInt8.self), count: packet.count)
        input.packetDescriptions?.pointee = AudioStreamPacketDescription(mStartOffset: 0, mVariableFramesInPacket: 480, mDataByteSize: UInt32(packet.count))
        let output = try XCTUnwrap(AVAudioPCMBuffer(pcmFormat: pcm, frameCapacity: 960))
        var total: AVAudioFrameCount = 0
        for index in 0..<6 {
            var supplied = false, error: NSError?
            let status = converter.convert(to: output, error: &error) { _, state in
                if supplied { state.pointee = .noDataNow; return nil }
                supplied = true; state.pointee = .haveData; return input
            }
            XCTAssertNotEqual(status, .error, "\(String(describing: error))")
            // The Apple decoder consumes its initial 120-sample priming delay.
            if index > 0 { XCTAssertEqual(output.frameLength, 480) }
            total += output.frameLength
        }
        XCTAssertGreaterThanOrEqual(total, 6 * 480 - 480)
    }
    func testLetterboxZoomAndSourceMapping() {
        let fit = VideoGeometry(source: CGSize(width: 1920, height: 1080), bounds: CGSize(width: 1000, height: 800))
        XCTAssertNil(fit.sourcePoint(CGPoint(x: 500, y: 30)))
        XCTAssertEqual(fit.sourcePoint(CGPoint(x: 500, y: 400)), CGPoint(x: 960, y: 540))
        let zoom = VideoGeometry(source: fit.source, bounds: fit.bounds, zoom: 2, pan: CGPoint(x: 100, y: -50))
        let p = CGPoint(x: zoom.rect.midX, y: zoom.rect.midY)
        XCTAssertEqual(zoom.sourcePoint(p), CGPoint(x: 960, y: 540))
        XCTAssertNil(VideoGeometry(source: .zero, bounds: .zero).sourcePoint(.zero))
    }
    func testAnnexBHandlesMixedPrefixesAndEmptyUnits() {
        let data: [UInt8] = [0, 0, 0, 1, 0x67, 2, 0, 0, 1, 0x68, 3, 0, 0, 1]
        data.withUnsafeBufferPointer { bytes in
            let ranges = AnnexB.ranges(bytes)
            XCTAssertEqual(ranges.map { Array(bytes[$0]) }, [[0x67, 2], [0x68, 3]])
        }
    }
    func testNativeH264DecodeAndRestart() throws {
        let path = try XCTUnwrap(Bundle(for: Self.self).url(forResource: "frame", withExtension: "h264"))
        let data = try Data(contentsOf: path)
        var config = Data(), picture = Data()
        data.withUnsafeBytes { raw in
            let bytes = raw.bindMemory(to: UInt8.self)
            for range in AnnexB.ranges(bytes) {
                let unit = Data([0, 0, 0, 1]) + Data(bytes[range])
                if [7, 8].contains(bytes[range.lowerBound] & 31) { config += unit }
                else { picture += unit }
            }
        }
        XCTAssertFalse(config.isEmpty); XCTAssertFalse(picture.isEmpty)
        let decoder = VideoDecoder()
        decoder.failure = { status, _ in XCTFail("VideoToolbox OSStatus \(status)") }
        for generation: UInt64 in [1, 3] {
            let decoded = expectation(description: "decoded generation \(generation)")
            decoder.output = { buffer, actual in
                XCTAssertEqual(actual, generation)
                XCTAssertEqual(CVPixelBufferGetWidth(buffer), 320)
                XCTAssertEqual(CVPixelBufferGetHeight(buffer), 180)
                decoded.fulfill()
            }
            decoder.configure(generation: generation)
            XCTAssertTrue(config.withUnsafeBytes { decoder.submit($0.bindMemory(to: UInt8.self).baseAddress!, count: config.count, pts: 0, flags: 1, generation: generation) })
            XCTAssertTrue(picture.withUnsafeBytes { decoder.submit($0.bindMemory(to: UInt8.self).baseAddress!, count: picture.count, pts: 0, flags: 2, generation: generation) })
            wait(for: [decoded], timeout: 5)
            decoder.stop(generation: generation + 1)
            XCTAssertFalse(picture.withUnsafeBytes { decoder.submit($0.bindMemory(to: UInt8.self).baseAddress!, count: picture.count, pts: 0, flags: 2, generation: generation) })
        }
    }
}
