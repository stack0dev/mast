import SwiftUI
import SwiftData

struct ReflectionSheet: View {
    @Environment(\.dismiss) private var dismiss
    @Environment(MastBridge.self) private var bridge
    @Environment(\.modelContext) private var modelContext

    let entryContent: String

    @State private var reflectionText = ""
    @State private var isGenerating = false
    @State private var error: String?

    var body: some View {
        NavigationStack {
            VStack(alignment: .leading, spacing: 0) {
                header

                Divider()

                reflectionBody
            }
            .navigationTitle("Reflection")
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") {
                        dismiss()
                    }
                }
            }
            .task {
                await generateReflection()
            }
        }
        .frame(minWidth: 480, minHeight: 350)
    }

    // MARK: - Header

    private var header: some View {
        VStack(alignment: .leading, spacing: 4) {
            Label("AI Reflection", systemImage: "sparkles")
                .font(.headline)
                .foregroundStyle(.tint)

            Text("Based on your entry and related memories")
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .padding()
    }

    // MARK: - Body

    @ViewBuilder
    private var reflectionBody: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                if let error {
                    errorView(error)
                } else if reflectionText.isEmpty, isGenerating {
                    HStack {
                        ProgressView()
                            .controlSize(.small)
                        Text("Reflecting...")
                            .foregroundStyle(.secondary)
                    }
                    .padding()
                } else {
                    Text(MarkdownHelper.render(reflectionText))
                        .textSelection(.enabled)
                        .padding()

                    if isGenerating {
                        ProgressView()
                            .controlSize(.small)
                            .padding(.horizontal)
                    }
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private func errorView(_ message: String) -> some View {
        VStack(spacing: 8) {
            Image(systemName: "exclamationmark.triangle")
                .font(.title2)
                .foregroundStyle(.orange)
            Text(message)
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
            Button("Try Again") {
                error = nil
                Task { await generateReflection() }
            }
            .buttonStyle(.bordered)
        }
        .padding()
        .frame(maxWidth: .infinity)
    }

    // MARK: - Generation

    private func generateReflection() async {
        isGenerating = true
        defer { isGenerating = false }

        // Fetch related memories for context
        let embedder = AppleEmbedder()
        var relatedContents: [String] = []

        do {
            let results = try await bridge.recall(
                collection: "journal",
                query: entryContent,
                textQuery: nil,
                mode: .vector,
                limit: 5,
                filterJSON: nil,
                embed: { text in await embedder.embed(text) },
                dimensions: embedder.dimensions
            )
            relatedContents = results.map(\.content)
        } catch {
            // Continue without related memories -- reflection still works
        }

        // Build the prompt
        let prompts = PromptTemplates.reflection(
            entry: entryContent,
            relatedMemories: relatedContents
        )

        // Resolve the LLM provider from settings
        let provider = resolveProvider()

        do {
            let stream = try await provider.generateStream(
                systemPrompt: prompts.system,
                userMessage: prompts.user
            )
            for try await chunk in stream {
                reflectionText += chunk
            }
        } catch {
            if reflectionText.isEmpty {
                self.error = error.localizedDescription
            }
            // If we already have partial text, just stop gracefully
        }
    }

    private func resolveProvider() -> any LLMProvider {
        let descriptor = FetchDescriptor<Settings>()
        let settings = try? modelContext.fetch(descriptor).first

        switch settings?.llmProvider ?? "apple" {
        case "openai":
            let key = settings?.openAIKey ?? ""
            let model = settings?.openAIModel ?? "gpt-4o-mini"
            guard !key.isEmpty else { return AppleLLMProvider() }
            return OpenAIProvider(apiKey: key, model: model)
        case "claude":
            let key = settings?.claudeKey ?? ""
            let model = settings?.claudeModel ?? "claude-sonnet-4-5-20250929"
            guard !key.isEmpty else { return AppleLLMProvider() }
            return ClaudeProvider(apiKey: key, model: model)
        default:
            return AppleLLMProvider()
        }
    }
}
