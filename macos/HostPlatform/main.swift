// Display metadata only. Does not capture the screen or request privacy access.
import Foundation
import CoreGraphics

do {
    let args = Array(CommandLine.arguments.dropFirst())
    guard args.count == 3, args[0] == "display", args[1] == "--output" else { throw Failure.invalidArguments }
    let id: CGDirectDisplayID
    if args[2].isEmpty { id = CGMainDisplayID() }
    else { guard let value = UInt32(args[2]) else { throw Failure.invalidDisplay }; id = value }
    guard CGDisplayIsActive(id) != 0, let mode = CGDisplayCopyDisplayMode(id), mode.pixelWidth >= 320, mode.pixelHeight >= 240 else { throw Failure.invalidDisplay }
    // Match Sunshine av_video.m: CGDisplayModeGetPixelWidth/Height, not points.
    let data = try JSONSerialization.data(withJSONObject: ["displayId": id, "width": mode.pixelWidth, "height": mode.pixelHeight], options: .sortedKeys)
    FileHandle.standardOutput.write(data + Data([10]))
} catch {
    FileHandle.standardError.write(Data("Mac capture display metadata unavailable\n".utf8))
    exit(1)
}
enum Failure: Error { case invalidArguments, invalidDisplay }
