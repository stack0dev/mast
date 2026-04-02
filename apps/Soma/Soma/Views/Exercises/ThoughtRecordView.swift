import SwiftUI
import SwiftData

struct ThoughtRecordView: View {
    @Environment(\.dismiss) private var dismiss
    @Environment(MastBridge.self) private var bridge
    @Environment(\.modelContext) private var modelContext

    // CBT fields
    @State private var situation = ""
    @State private var automaticThought = ""
    @State private var emotion = ""
    @State private var emotionIntensity: Double = 5
    @State private var evidenceFor = ""
    @State private var evidenceAgainst = ""
    @State private var balancedThought = ""
    @State private var newEmotion = ""
    @State private var newEmotionIntensity: Double = 5

    // UI state
    @State private var isSaving = false
    @State private var isReframing = false
    @State private var reframeText = ""
    @State private var error: String?
    @State private var selectedMood: Mood = .neutral

    private var embedder: AppleEmbedder { AppleEmbedder() }

    private var canSave: Bool {
        !situation.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty &&
        !automaticThought.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty &&
        !isSaving
    }

    var body: some View {
        NavigationStack {
            Form {
                situationSection
                thoughtSection
                emotionSection
                evidenceSection
                balancedThoughtSection
                newEmotionSection

                if !reframeText.isEmpty {
                    reframeSection
                }
            }
            .formStyle(.grouped)
            .navigationTitle("Thought Record")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                        .disabled(isSaving)
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") { save() }
                        .disabled(!canSave)
                }
            }
            .alert("Error", isPresented: .init(
                get: { error != nil },
                set: { if !$0 { error = nil } }
            )) {
                Button("OK") { error = nil }
            } message: {
                Text(error ?? "")
            }
        }
        .frame(minWidth: 550, minHeight: 600)
    }

    // MARK: - Sections

    private var situationSection: some View {
        Section {
            TextField("What happened? Describe the situation.", text: $situation, axis: .vertical)
                .lineLimit(3...6)
        } header: {
            Label("Situation", systemImage: "location")
        }
    }

    private var thoughtSection: some View {
        Section {
            TextField("What went through your mind?", text: $automaticThought, axis: .vertical)
                .lineLimit(3...6)

            Button {
                reframe()
            } label: {
                Label(
                    isReframing ? "Reframing..." : "Help Me Reframe",
                    systemImage: "sparkles"
                )
            }
            .disabled(automaticThought.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || isReframing)
        } header: {
            Label("Automatic Thought", systemImage: "brain")
        }
    }

    private var emotionSection: some View {
        Section {
            TextField("e.g., anxious, sad, frustrated", text: $emotion)

            VStack(alignment: .leading) {
                Text("Intensity: \(Int(emotionIntensity))/10")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                Slider(value: $emotionIntensity, in: 0...10, step: 1) {
                    Text("Intensity")
                }
            }
        } header: {
            Label("Emotion", systemImage: "heart")
        }
    }

    private var evidenceSection: some View {
        Group {
            Section {
                TextField("What facts support this thought?", text: $evidenceFor, axis: .vertical)
                    .lineLimit(3...6)
            } header: {
                Label("Evidence For", systemImage: "checkmark.circle")
            }

            Section {
                TextField("What facts contradict this thought?", text: $evidenceAgainst, axis: .vertical)
                    .lineLimit(3...6)
            } header: {
                Label("Evidence Against", systemImage: "xmark.circle")
            }
        }
    }

    private var balancedThoughtSection: some View {
        Section {
            TextField("A more balanced way to look at this...", text: $balancedThought, axis: .vertical)
                .lineLimit(3...6)
        } header: {
            Label("Balanced Thought", systemImage: "scale.3d")
        }
    }

    private var newEmotionSection: some View {
        Section {
            TextField("How do you feel now?", text: $newEmotion)

            VStack(alignment: .leading) {
                Text("New Intensity: \(Int(newEmotionIntensity))/10")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                Slider(value: $newEmotionIntensity, in: 0...10, step: 1) {
                    Text("New Intensity")
                }
            }

            Picker("Overall Mood", selection: $selectedMood) {
                ForEach(Mood.allCases) { mood in
                    Text("\(mood.emoji) \(mood.rawValue.capitalized)").tag(mood)
                }
            }
        } header: {
            Label("After Reflecting", systemImage: "arrow.triangle.2.circlepath")
        }
    }

    private var reframeSection: some View {
        Section {
            Text(MarkdownHelper.render(reframeText))
                .textSelection(.enabled)
        } header: {
            Label("AI Reframe Suggestion", systemImage: "sparkles")
        }
    }

    // MARK: - Actions

    private func reframe() {
        isReframing = true
        let prompts = PromptTemplates.reframe(thought: automaticThought)

        let provider = resolveProvider()
        Task {
            do {
                let stream = try await provider.generateStream(
                    systemPrompt: prompts.system,
                    userMessage: prompts.user
                )
                reframeText = ""
                for try await chunk in stream {
                    reframeText += chunk
                }
            } catch {
                self.error = error.localizedDescription
            }
            isReframing = false
        }
    }

    private func save() {
        isSaving = true
        let content = """
        ## Thought Record

        **Situation:** \(situation)

        **Automatic Thought:** \(automaticThought)

        **Emotion:** \(emotion) (intensity: \(Int(emotionIntensity))/10)

        **Evidence For:** \(evidenceFor)

        **Evidence Against:** \(evidenceAgainst)

        **Balanced Thought:** \(balancedThought)

        **After Reflecting:** \(newEmotion) (intensity: \(Int(newEmotionIntensity))/10)
        """

        let emb = embedder
        let vm = ExerciseViewModel(bridge: bridge)
        Task {
            do {
                try await vm.saveExercise(
                    type: "thought_record",
                    content: content,
                    mood: selectedMood.rawValue,
                    embed: { text in await emb.embed(text) },
                    dimensions: emb.dimensions
                )

                // Record mood
                let record = MoodRecord(mood: selectedMood.rawValue, entryId: "")
                modelContext.insert(record)
                try? modelContext.save()

                let streakService = StreakService(modelContext: modelContext)
                streakService.recordEntry()

                isSaving = false
                dismiss()
            } catch {
                self.error = error.localizedDescription
                isSaving = false
            }
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
