import Foundation
import SwiftData

@Observable
final class JournalViewModel {
    var entries: [MastMemory] = []
    var searchResults: [MastRecallResult] = []
    var searchQuery = ""
    var isSearching = false
    var isLoading = false
    var error: String?

    private let bridge: MastBridge
    private let collection = "journal"

    init(bridge: MastBridge) {
        self.bridge = bridge
    }

    func loadEntries() async {
        isLoading = true
        defer { isLoading = false }
        do {
            entries = try await bridge.list(collection: collection)
            entries.sort { $0.createdAt > $1.createdAt }
        } catch {
            self.error = error.localizedDescription
        }
    }

    func search(embed: @escaping (String) async -> [Float], dimensions: Int) async {
        guard !searchQuery.trimmingCharacters(in: .whitespaces).isEmpty else {
            searchResults = []
            isSearching = false
            return
        }
        isSearching = true
        do {
            searchResults = try await bridge.recall(
                collection: collection,
                query: searchQuery,
                textQuery: searchQuery,
                mode: .hybrid(vectorWeight: 0.5, textWeight: 0.5),
                limit: 20,
                filterJSON: nil,
                embed: embed,
                dimensions: dimensions
            )
        } catch {
            self.error = error.localizedDescription
        }
    }

    func saveEntry(
        content: String,
        mood: String,
        metadata: [String: String],
        embed: @escaping (String) async -> [Float],
        dimensions: Int
    ) async throws -> String {
        let plainText = MarkdownHelper.stripFormatting(content)
        var meta = metadata
        meta["raw_markdown"] = content
        meta["mood"] = mood
        meta["date"] = ISO8601DateFormatter().string(from: .now)
        let id = try await bridge.store(
            collection: collection,
            content: plainText,
            metadata: meta,
            tier: .active,
            ttl: nil,
            embed: embed,
            dimensions: dimensions
        )
        await loadEntries()
        return id
    }

    func deleteEntry(id: String) async throws {
        try await bridge.delete(collection: collection, id: id)
        await loadEntries()
    }
}
