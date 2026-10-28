import SwiftUI

@main
struct KiteApp: App {
    init() {
        initLogging()
    }

    var body: some Scene {
        WindowGroup {
            HostCheckView()
        }
    }
}
