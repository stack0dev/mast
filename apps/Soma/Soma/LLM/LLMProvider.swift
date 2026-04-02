import Foundation

// MARK: - LLM Provider Protocol

protocol LLMProvider: Sendable {
    var displayName: String { get }
    func generate(systemPrompt: String, userMessage: String) async throws -> String
    func generateStream(systemPrompt: String, userMessage: String) async throws -> AsyncThrowingStream<String, Error>
}

// MARK: - Errors

enum LLMError: LocalizedError {
    case invalidURL
    case httpError(statusCode: Int, body: String)
    case decodingError(String)
    case emptyResponse
    case streamingError(String)
    case unavailable(String)

    var errorDescription: String? {
        switch self {
        case .invalidURL:
            "Invalid API URL."
        case .httpError(let statusCode, let body):
            "HTTP \(statusCode): \(body)"
        case .decodingError(let detail):
            "Failed to decode response: \(detail)"
        case .emptyResponse:
            "The model returned an empty response."
        case .streamingError(let detail):
            "Streaming error: \(detail)"
        case .unavailable(let reason):
            "Provider unavailable: \(reason)"
        }
    }
}
