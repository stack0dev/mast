import SwiftUI
import SwiftData

struct JournalEditorView: View {
    @Environment(\.dismiss) private var dismiss
    @Environment(\.modelContext) private var modelContext
    @Environment(MastBridge.self) private var bridge

    let viewModel: JournalViewModel

    @State private var content = ""
    @State private var selectedMood: Mood = .neutral
    @State private var isSaving = false
    @State private var savedEntryContent: String?
    @State private var showReflection = false
    @State private var error: String?

    private var embedder: AppleEmbedder {
        AppleEmbedder()
    }

    private var canSave: Bool {
        !content.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && !isSaving
    }

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                moodPicker

                Divider()

                editor
            }
            .navigationTitle("New Entry")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") {
                        dismiss()
                    }
                    .disabled(isSaving)
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") {
                        save()
                    }
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
            .sheet(isPresented: $showReflection) {
                if let savedContent = savedEntryContent {
                    ReflectionSheet(entryContent: savedContent)
                }
            }
        }
        .frame(minWidth: 500, minHeight: 400)
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

    private var editor: some View {
        ZStack(alignment: .topLeading) {
            TextEditor(text: $content)
                .font(.body)
                .scrollContentBackground(.hidden)
                .padding(8)

            if content.isEmpty {
                Text("What's on your mind today?")
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
        let emb = embedder
        Task {
            do {
                let id = try await viewModel.saveEntry(
                    content: content,
                    mood: selectedMood.rawValue,
                    metadata: [:],
                    embed: { text in await emb.embed(text) },
                    dimensions: emb.dimensions
                )

                // Record mood in SwiftData
                let record = MoodRecord(
                    mood: selectedMood.rawValue,
                    entryId: id
                )
                modelContext.insert(record)
                try? modelContext.save()

                // Update streak
                let streakService = StreakService(modelContext: modelContext)
                streakService.recordEntry()

                savedEntryContent = content
                isSaving = false
                showReflection = true
            } catch {
                self.error = error.localizedDescription
                isSaving = false
            }
        }
    }
}
