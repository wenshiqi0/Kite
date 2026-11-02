import AppKit
import Observation

struct LocalHostReport: Decodable {
    struct Dependencies: Decodable { let herdr: String?; let bun: String?; let sunshine: String? }
    struct Node: Decodable {
        struct Service: Decodable, Identifiable { var id: String { unit }; let name: String; let unit: String; let active: Bool; let ownership: String; let error: String? }
        let nodeId: String; let machine: String; let version: String; let health: String; let pid: Int; let services: [Service]
    }
    let configured: Bool
    let hostname: String
    let username: String
    let sshPort: Int
    let sshAvailable: Bool
    let dependencies: Dependencies
    let hostApplication: String?
    let herdrRunning: Bool
    let desktopAvailable: Bool
    let chatAvailable: Bool
    let node: Node?
    let resourceLimitsAvailable: Bool
}

@MainActor @Observable
final class LocalHostModel {
    private(set) var report: LocalHostReport?
    private(set) var refreshing = false
    private(set) var busy = false
    private(set) var error: String?
    private(set) var updatedAt: Date?
    private var operationError: String?
    @ObservationIgnored private var monitor: Task<Void, Never>?
    init() {
        guard ProcessInfo.processInfo.environment["XCTestConfigurationFilePath"] == nil else { return }
        monitor = Task { [weak self] in
            while !Task.isCancelled {
                await self?.refresh()
                do { try await Task.sleep(for: .seconds(15)) } catch { return }
            }
        }
    }
    var statusLabel: String { report?.chatAvailable == true ? tr("host_chat_online") : tr("host_not_running") }
    var online: Bool { report?.chatAvailable == true }

    private static func invoke(_ args: [String]) async throws -> Data {
        guard let resources = Bundle.main.resourceURL else { throw HostFailure.message("Host components are missing") }
        let executable = resources.appendingPathComponent("host/Kite Host.app/Contents/MacOS/kite-node")
        return try await Task.detached(priority: .utility) {
            let process = Process(), output = Pipe(), errors = Pipe()
            process.executableURL = executable
            process.arguments = ["host"] + args
            process.standardInput = FileHandle.nullDevice
            process.standardOutput = output
            process.standardError = errors
            try process.run()
            let timeout = DispatchWorkItem { if process.isRunning { process.terminate() } }
            DispatchQueue.global(qos: .utility).asyncAfter(deadline: .now() + 45, execute: timeout)
            defer { timeout.cancel() }
            let data = output.fileHandleForReading.readDataToEndOfFile()
            let problem = errors.fileHandleForReading.readDataToEndOfFile()
            process.waitUntilExit()
            guard data.count <= 1024 * 1024, problem.count <= 16 * 1024 else { throw HostFailure.message("Host response exceeded its size limit") }
            guard process.terminationStatus == 0 else {
                throw HostFailure.message(String(data: problem, encoding: .utf8)?.trimmingCharacters(in: .whitespacesAndNewlines).prefix(1000).description ?? "Host command failed")
            }
            return data
        }.value
    }
    func refresh() async {
        guard !refreshing, !busy else { return }
        refreshing = true
        defer { refreshing = false }
        do {
            report = try JSONDecoder().decode(LocalHostReport.self, from: await Self.invoke(["inspect"]))
            updatedAt = Date(); error = operationError
        } catch {
            report = nil; updatedAt = nil; self.error = error.localizedDescription
        }
    }
    func run(_ action: String) {
        guard !busy, !refreshing else { return }
        busy = true; operationError = nil; error = nil
        Task {
            do {
                if action == "install" { _ = try await Self.invoke(["install"]); _ = try await Self.invoke(["enable"]) }
                else { _ = try await Self.invoke([action]) }
            } catch { operationError = error.localizedDescription; self.error = operationError }
            busy = false
            await refresh()
        }
    }
    func clearError() { operationError = nil; error = nil }
    func poll() async {
        while !Task.isCancelled {
            await refresh()
            do { try await Task.sleep(for: .seconds(5)) } catch { return }
        }
    }
    private enum HostFailure: LocalizedError { case message(String); var errorDescription: String? { if case let .message(text) = self { return text }; return nil } }
}
