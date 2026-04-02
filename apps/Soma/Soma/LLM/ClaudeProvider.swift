import Foundation

// MARK: - Claude Provider

final class ClaudeProvider: LLMProvider, @unchecked Sendable {
    private let apiKey: String
    private let model: String
    private let maxTokens: Int
    private let session: URLSession
    let displayName = "Claude"

    private static let endpoint = URL(string: "https://api.anthropic.com/v1/messages")!
    private static let apiVersion = "2023-06-01"

    init(apiKey: String, model: String = "claude-sonnet-4-5-20250929", maxTokens: Int = 1024) {
        self.apiKey = apiKey
        self.model = model
        self.maxTokens = maxTokens
        self.session = URLSession(configuration: .ephemeral)
    }

    // MARK: - Non-Streaming

    func generate(systemPrompt: String, userMessage: String) async throws -> String {
        let body = MessagesRequest(
            model: model,
            max_tokens: maxTokens,
            system: systemPrompt,
            messages: [.init(role: "user", content: userMessage)],
            stream: false
        )

        let request = try buildRequest(body: body)
        let (data, response) = try await session.data(for: request)
        try validateHTTPResponse(response, data: data)

        let decoded = try JSONDecoder().decode(MessagesResponse.self, from: data)
        guard let block = decoded.content.first, case .text(let text) = block, !text.isEmpty else {
            throw LLMError.emptyResponse
        }
        return text
    }

    // MARK: - Streaming

    func generateStream(
        systemPrompt: String,
        userMessage: String
    ) async throws -> AsyncThrowingStream<String, Error> {
        let body = MessagesRequest(
            model: model,
            max_tokens: maxTokens,
            system: systemPrompt,
            messages: [.init(role: "user", content: userMessage)],
            stream: true
        )

        let request = try buildRequest(body: body)
        let (bytes, response) = try await session.bytes(for: request)
        try validateHTTPResponse(response, data: nil)

        return AsyncThrowingStream { continuation in
            Task {
                do {
                    var currentEvent: String?
                    for try await line in bytes.lines {
                        if line.hasPrefix("event: ") {
                            currentEvent = String(line.dropFirst(7))
                            continue
                        }

                        guard line.hasPrefix("data: "), let event = currentEvent else { continue }

                        if event == "message_stop" { break }

                        guard event == "content_block_delta" else { continue }
                        let payload = String(line.dropFirst(6))
                        guard let chunk = payload.data(using: .utf8) else { continue }

                        let decoded = try JSONDecoder().decode(ContentBlockDelta.self, from: chunk)
                        if let text = decoded.delta.text {
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

    private func buildRequest(body: MessagesRequest) throws -> URLRequest {
        var request = URLRequest(url: Self.endpoint)
        request.httpMethod = "POST"
        request.setValue(apiKey, forHTTPHeaderField: "x-api-key")
        request.setValue(Self.apiVersion, forHTTPHeaderField: "anthropic-version")
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

private extension ClaudeProvider {
    struct MessagesRequest: Encodable {
        let model: String
        let max_tokens: Int
        let system: String
        let messages: [Message]
        let stream: Bool
    }

    struct Message: Encodable {
        let role: String
        let content: String
    }

    struct MessagesResponse: Decodable {
        let content: [ContentBlock]
    }

    enum ContentBlock: Decodable {
        case text(String)

        private enum CodingKeys: String, CodingKey {
            case type, text
        }

        init(from decoder: Decoder) throws {
            let container = try decoder.container(keyedBy: CodingKeys.self)
            let type = try container.decode(String.self, forKey: .type)
            switch type {
            case "text":
                let text = try container.decode(String.self, forKey: .text)
                self = .text(text)
            default:
                throw LLMError.decodingError("Unknown content block type: \(type)")
            }
        }
    }

    struct ContentBlockDelta: Decodable {
        let delta: Delta

        struct Delta: Decodable {
            let text: String?
        }
    }
}
