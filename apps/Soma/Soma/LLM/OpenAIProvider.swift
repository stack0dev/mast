import Foundation

// MARK: - OpenAI Provider

final class OpenAIProvider: LLMProvider, @unchecked Sendable {
    private let apiKey: String
    private let model: String
    private let session: URLSession
    let displayName = "OpenAI"

    private static let endpoint = URL(string: "https://api.openai.com/v1/chat/completions")!

    init(apiKey: String, model: String = "gpt-4o-mini") {
        self.apiKey = apiKey
        self.model = model
        self.session = URLSession(configuration: .ephemeral)
    }

    // MARK: - Non-Streaming

    func generate(systemPrompt: String, userMessage: String) async throws -> String {
        let body = CompletionRequest(
            model: model,
            messages: [
                .init(role: "system", content: systemPrompt),
                .init(role: "user", content: userMessage),
            ],
            stream: false
        )

        let request = try buildRequest(body: body)
        let (data, response) = try await session.data(for: request)
        try validateHTTPResponse(response, data: data)

        let decoded = try JSONDecoder().decode(CompletionResponse.self, from: data)
        guard let text = decoded.choices.first?.message.content, !text.isEmpty else {
            throw LLMError.emptyResponse
        }
        return text
    }

    // MARK: - Streaming

    func generateStream(
        systemPrompt: String,
        userMessage: String
    ) async throws -> AsyncThrowingStream<String, Error> {
        let body = CompletionRequest(
            model: model,
            messages: [
                .init(role: "system", content: systemPrompt),
                .init(role: "user", content: userMessage),
            ],
            stream: true
        )

        let request = try buildRequest(body: body)
        let (bytes, response) = try await session.bytes(for: request)
        try validateHTTPResponse(response, data: nil)

        return AsyncThrowingStream { continuation in
            Task {
                do {
                    for try await line in bytes.lines {
                        guard line.hasPrefix("data: ") else { continue }
                        let payload = String(line.dropFirst(6))
                        if payload == "[DONE]" { break }

                        guard let chunk = payload.data(using: .utf8) else { continue }
                        let decoded = try JSONDecoder().decode(StreamChunk.self, from: chunk)
                        if let text = decoded.choices.first?.delta.content {
                            continuation.yield(text)
                        }
                    }
                    continuation.finish()
                } catch {
                    continuation.finish(throwing: error)
                }
            }
        }
    }

    // MARK: - Request Building

    private func buildRequest(body: CompletionRequest) throws -> URLRequest {
        var request = URLRequest(url: Self.endpoint)
        request.httpMethod = "POST"
        request.setValue("Bearer \(apiKey)", forHTTPHeaderField: "Authorization")
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try JSONEncoder().encode(body)
        return request
    }

    private func validateHTTPResponse(_ response: URLResponse, data: Data?) throws {
        guard let http = response as? HTTPURLResponse else { return }
        guard (200...299).contains(http.statusCode) else {
            let body = data.flatMap { String(data: $0, encoding: .utf8) } ?? "No body"
            throw LLMError.httpError(statusCode: http.statusCode, body: body)
        }
    }
}

// MARK: - Request / Response Types

private extension OpenAIProvider {
    struct CompletionRequest: Encodable {
        let model: String
        let messages: [Message]
        let stream: Bool
    }

    struct Message: Codable {
        let role: String
        let content: String
    }

    struct CompletionResponse: Decodable {
        let choices: [Choice]

        struct Choice: Decodable {
            let message: Message
        }
    }

    struct StreamChunk: Decodable {
        let choices: [StreamChoice]

        struct StreamChoice: Decodable {
            let delta: Delta
        }

        struct Delta: Decodable {
            let content: String?
        }
    }
}
