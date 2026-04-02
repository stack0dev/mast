import SwiftUI
import SwiftData

struct GratitudeView: View {
    @Environment(\.dismiss) private var dismiss
    @Environment(MastBridge.self) private var bridge
    @Environment(\.modelContext) private var modelContext

    @State private var prompt: String = PromptTemplates.gratitudePrompt()
    @State private var response = ""
    @State private var selectedMood: Mood = .calm
    @State private var isSaving = false
    @State private var error: String?

    private var embedder: AppleEmbedder { AppleEmbedder() }

    private var canSave: Bool {
        !response.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && !isSaving
    }

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                promptCard

                Divider()

                moodPicker

                Divider()

                editorArea
            }
            .navigationTitle("Gratitude")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                        .disabled(isSaving)
                }
                ToolbarItem(placement: .primaryAction) {
                    Button {
                        prompt = PromptTemplates.gratitudePrompt()
                    } label: {
                        Label("New Prompt", systemImage: "arrow.clockwise")
                    }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") { save() }
                        .disabled(!canSave)
                        .keyboardShortcut(.return, modifiers: .command)
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
        .frame(minWidth: 480, minHeight: 400)
    }

    // MARK: - Prompt Card

    private var promptCard: some View {
        HStack(spacing: 12) {
            Image(systemName: "sparkle")
                .font(.title3)
                .foregroundStyle(.orange)

            Text(prompt)
                .font(.title3)
                .fontWeight(.medium)
                .multilineTextAlignment(.leading)

            Spacer()
        }
        .padding()
        .background(.orange.opacity(0.06))
    }

    // MARK: - Mood Picker

    private var moodPicker: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                ForEach(Mood.allCases) { mood in
                    Button {
                        withAnimation(.snappy(duration: 0.2)) {
                            selectedMood = mood
                        }
                    } label: {
                        HStack(spacing: 4) {
                            Text(mood.emoji)
                            Text(mood.rawValue.capitalized)
                                .font(.subheadline)
                        }
                        .padding(.horizontal, 12)
                        .padding(.vertical, 8)
                        .background(
                            selectedMood == mood
                                ? mood.color.opacity(0.25)
                                : Color.secondary.opacity(0.08),
                            in: Capsule()
                        )
                        .overlay(
                            Capsule()
                                .strokeBorder(
                                    selectedMood == mood ? mood.color : .clear,
                                    lineWidth: 1.5
                                )
                        )
                    }
                    .buttonStyle(.plain)
                }
            }
            .padding(.horizontal)
            .padding(.vertical, 10)
        }
    }

    // MARK: - Editor

    private var editorArea: some View {
        ZStack(alignment: .topLeading) {
            TextEditor(text: $response)
                .font(.body)
                .scrollContentBackground(.hidden)
                .padding(8)

            if response.isEmpty {
                Text("Write your gratitude here...")
                    .foregroundStyle(.tertiary)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 16)
                    .allowsHitTesting(false)
            }
        }
        .overlay {
            if isSaving {
                ZStack {
                    Color.black.opacity(0.05)
                    ProgressView("Saving...")
                        .padding()
                        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 10))
                }
            }
        }
    }

    // MARK: - Save

    private func save() {
        isSaving = true
        let content = """
        ## Gratitude

        **Prompt:** \(prompt)

        \(response)
        """

        let emb = embedder
        let vm = ExerciseViewModel(bridge: bridge)
        Task {
            do {
                try await vm.saveExercise(
                    type: "gratitude",
                    content: content,
                    mood: selectedMood.rawValue,
                    embed: { text in await emb.embed(text) },
                    dimensions: emb.dimensions
                )

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
}
