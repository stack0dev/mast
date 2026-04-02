import Foundation

// MARK: - Apple On-Device Embedder

/// Uses the Foundation Models FeatureExtractor available in macOS 26+.
/// This runs entirely on-device with no network dependency.
@available(macOS 26.0, iOS 26.0, *)
struct AppleEmbedder: Sendable {

    func embed(_ text: String) async -> [Float] {
        do {
            let extractor = FeatureExtractor()
            let embedding = try await extractor.embed(text)
            return embedding.vector.map { Float($0) }
        } catch {
            return []
        }
    }

    var dimensions: Int {
        // Apple's on-device embedding model dimension.
        // FeatureExtractor uses a 512-dimensional space.
        512
    }
}

// MARK: - OpenAI Embedder

/// Calls the OpenAI embeddings API over the network.
/// Requires a valid API key. Uses text-embedding-3-small by default.
struct OpenAIEmbedder: Sendable {
    let apiKey: String
    let model: String
    let dimensions: Int

    init(apiKey: String, model: String = "text-embedding-3-small", dimensions: Int = 1536) {
        self.apiKey = apiKey
        self.model = model
        self.dimensions = dimensions
    }

    func embed(_ text: String) async -> [Float] {
        let url = URL(string: "https://api.openai.com/v1/embeddings")!
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue("Bearer \(apiKey)", forHTTPHeaderField: "Authorization")
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")

        let body: [String: Any] = [
            "input": text,
            "model": model,
            "dimensions": dimensions,
        ]

        guard let httpBody = try? JSONSerialization.data(withJSONObject: body) else {
            return []
        }
        request.httpBody = httpBody

        do {
            let (data, response) = try await URLSession.shared.data(for: request)
            guard let httpResponse = response as? HTTPURLResponse,
                  httpResponse.statusCode == 200
            else {
                return []
            }
            return parseEmbeddingResponse(data)
        } catch {
            return []
        }
    }

    private func parseEmbeddingResponse(_ data: Data) -> [Float] {
        // Response shape: { "data": [{ "embedding": [0.1, 0.2, ...] }] }
        guard let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let dataArray = json["data"] as? [[String: Any]],
              let first = dataArray.first,
              let embedding = first["embedding"] as? [NSNumber]
        else {
            return []
        }
        return embedding.map { $0.floatValue }
    }
}
