import Foundation
import FoundationModels

// MARK: - Apple Intelligence Provider

@available(macOS 26.0, iOS 26.0, *)
final class AppleLLMProvider: LLMProvider {
    let displayName = "Apple Intelligence"

    func generate(systemPrompt: String, userMessage: String) async throws -> String {
        let session = LanguageModelSession(instructions: systemPrompt)
        let response = try await session.respond(to: userMessage)
        let text = response.content
        guard !text.isEmpty else { throw LLMError.emptyResponse }
        return text
    }

    func generateStream(
        systemPrompt: String,
        userMessage: String
    ) async throws -> AsyncThrowingStream<String, Error> {
        AsyncThrowingStream { continuation in
            let sendableSystem = systemPrompt
            let sendableUser = userMessage
            Task {
                do {
                    let session = LanguageModelSession(instructions: sendableSystem)
                    let stream = session.streamResponse(to: sendableUser)
                    for try await partial in stream {
                        if !partial.content.isEmpty {
                            continuation.yield(partial.content)
                        }
                    }
                    continuation.finish()
                } catch {
                    continuation.finish(throwing: error)
                }
            }
        }
    }
}
