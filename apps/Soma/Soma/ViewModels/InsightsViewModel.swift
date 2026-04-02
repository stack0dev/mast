import Foundation
import SwiftData

@Observable
final class InsightsViewModel {
    var patterns: [MastMemory] = []
    var moods: [MoodRecord] = []
    var edges: [MastEdge] = []
    var isLoading = false
    var error: String?

    private let bridge: MastBridge

    init(bridge: MastBridge) {
        self.bridge = bridge
    }

    func load(modelContext: ModelContext) async {
        isLoading = true
        defer { isLoading = false }

        // Load all journal memories and filter to archive tier (compacted pattern summaries)
        if let all = try? await bridge.list(collection: "journal") {
            patterns = all
                .filter { $0.tier == .archive }
                .sorted { $0.createdAt > $1.createdAt }
        }

        // Load mood records from SwiftData
        let descriptor = FetchDescriptor<MoodRecord>(
            sortBy: [SortDescriptor(\.date, order: .reverse)]
        )
        moods = (try? modelContext.fetch(descriptor)) ?? []

        // Load the trigger-emotion graph
        edges = (try? await bridge.subgraph(collection: "journal")) ?? []
    }
}
