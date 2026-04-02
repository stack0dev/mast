import SwiftUI
import SwiftData

@main
struct SomaApp: App {
    let modelContainer: ModelContainer
    let bridge: MastBridge

    init() {
        let schema = Schema([MoodRecord.self, Streak.self, Settings.self])
        modelContainer = try! ModelContainer(for: schema)
        bridge = MastBridge()

        let appSupport = FileManager.default.urls(
            for: .applicationSupportDirectory,
            in: .userDomainMask
        ).first!
        let somaDir = appSupport.appendingPathComponent("Soma", isDirectory: true)
        try? FileManager.default.createDirectory(at: somaDir, withIntermediateDirectories: true)
        let dbPath = somaDir.appendingPathComponent("mast.db").path

        let capturedBridge = bridge
        Task {
            try? await capturedBridge.open(path: dbPath)
        }
    }

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environment(bridge)
        }
        .modelContainer(modelContainer)
    }
}
