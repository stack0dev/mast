import Foundation

@Observable
final class PatternDetector {
    private let bridge: MastBridge
    private let llmProvider: any LLMProvider

    init(bridge: MastBridge, llmProvider: any LLMProvider) {
        self.bridge = bridge
        self.llmProvider = llmProvider
    }

    func detectPatterns(
        collection: String = "journal",
        strategy: MastCompactionStrategy = .age(seconds: 30 * 86400),
        maxBatchSize: Int = 10,
        embed: @escaping (String) async -> [Float],
        dimensions: Int
    ) async throws -> MastCompactResult {
        try await bridge.compact(
            collection: collection,
            sourceTier: .active,
            targetTier: .archive,
            strategy: strategy,
            maxBatchSize: maxBatchSize,
            dryRun: false,
            synthesize: { [llmProvider] contents in
                let prompts = PromptTemplates.patternSummary(entries: contents)
                return (try? await llmProvider.generate(
                    systemPrompt: prompts.system,
                    userMessage: prompts.user
                )) ?? contents.joined(separator: "\n---\n")
            },
            embed: embed,
            dimensions: dimensions
        )
    }
}
