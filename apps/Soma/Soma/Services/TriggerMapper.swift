import Foundation

struct TriggerEmotion: Codable {
    let trigger: String
    let emotion: String
    let intensity: Float
}

@Observable
final class TriggerMapper {
    private let bridge: MastBridge
    private let llmProvider: any LLMProvider

    init(bridge: MastBridge, llmProvider: any LLMProvider) {
        self.bridge = bridge
        self.llmProvider = llmProvider
    }

    func extractAndMap(entry: String, collection: String = "journal") async throws {
        let prompts = PromptTemplates.triggerExtraction(entry: entry)
        let response = try await llmProvider.generate(
            systemPrompt: prompts.system,
            userMessage: prompts.user
        )

        // Parse JSON response
        guard let data = response.data(using: .utf8),
              let triggers = try? JSONDecoder().decode([TriggerEmotion].self, from: data) else {
            return
        }

        for te in triggers {
            let _ = try await bridge.relate(
                collection: collection,
                source: te.trigger.lowercased(),
                target: te.emotion.lowercased(),
                relation: "triggers",
                weight: te.intensity
            )
        }
    }
}
