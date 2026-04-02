import Foundation

@Observable
final class ExerciseViewModel {
    var error: String?

    private let bridge: MastBridge

    init(bridge: MastBridge) {
        self.bridge = bridge
    }

    func saveExercise(
        type: String,
        content: String,
        mood: String,
        embed: @escaping (String) async -> [Float],
        dimensions: Int
    ) async throws {
        let plainText = MarkdownHelper.stripFormatting(content)
        let meta: [String: String] = [
            "raw_markdown": content,
            "mood": mood,
            "exercise_type": type,
            "date": ISO8601DateFormatter().string(from: .now),
        ]
        _ = try await bridge.store(
            collection: "journal",
            content: plainText,
            metadata: meta,
            tier: .active,
            ttl: nil,
            embed: embed,
            dimensions: dimensions
        )
    }
}
