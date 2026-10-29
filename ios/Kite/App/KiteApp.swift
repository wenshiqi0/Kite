import SwiftUI

@main
struct KiteApp: App {
    init() {
        try? initLogging()
    }

    var body: some Scene {
        WindowGroup {
            HostCheckView()
        }
    }
}
